use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{AccountId, AssetId, NetworkId};
use merchant_checkout_core::{
    CheckoutDefinition, CheckoutDispatchState, CheckoutError, CheckoutMutationOutcome,
    CheckoutState, FeeMode, MerchantId, MerchantOrderKey,
};
use merchant_checkout_lmdb::{
    LmdbMerchantCheckoutStore, MerchantCheckoutStoreError, MerchantCheckoutStoreOptions,
};
use payment_idempotency_core::{ReconciliationPageLimit, TenantId};

const NETWORK: NetworkId = NetworkId::new([11; 32]);
const TENANT: TenantId = TenantId::new([12; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "merchant-checkout-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn definition(order: u8) -> CheckoutDefinition {
    CheckoutDefinition {
        network: NETWORK,
        tenant: TENANT,
        merchant: MerchantId::new([13; 32]),
        order_key: MerchantOrderKey::new([order; 32]),
        merchant_account: AccountId::new([14; 32]),
        asset: AssetId::new([15; 32]),
        amount: 100,
        maximum_fee: 5,
        fee_mode: FeeMode::CustomerPays,
        expires_at_ms: 1_000,
        valid_until_height: 10_000,
    }
}

#[test]
fn immutable_order_claim_and_dispatch_queue_survive_restart() {
    let path = directory("restart");
    let first = definition(1);
    let second = definition(2);
    let payer = AccountId::new([16; 32]);
    {
        let store = LmdbMerchantCheckoutStore::open(
            &path,
            NETWORK,
            MerchantCheckoutStoreOptions::default(),
        )
        .unwrap();
        assert_eq!(
            store.create(first, 100).unwrap().outcome,
            CheckoutMutationOutcome::Applied
        );
        assert_eq!(
            store.create(first, 101).unwrap().outcome,
            CheckoutMutationOutcome::ExistingSame
        );
        let mut conflict = first;
        conflict.amount += 1;
        assert_eq!(
            store.create(conflict, 102),
            Err(MerchantCheckoutStoreError::Domain(
                CheckoutError::DefinitionConflict
            ))
        );
        store.create(second, 103).unwrap();
        store
            .claim(TENANT, first.checkout_id(), payer, 2, 104)
            .unwrap();
        store
            .claim(TENANT, second.checkout_id(), payer, 2, 105)
            .unwrap();
        let page = store
            .pending_dispatch_after(None, ReconciliationPageLimit::new(1).unwrap())
            .unwrap();
        assert_eq!(page.checkouts.len(), 1);
        assert!(page.next_cursor.is_some());
        store
            .mark_gateway_recorded(TENANT, first.checkout_id(), 106)
            .unwrap();
    }

    let reopened =
        LmdbMerchantCheckoutStore::open(&path, NETWORK, MerchantCheckoutStoreOptions::default())
            .unwrap();
    assert!(matches!(
        reopened
            .get(TENANT, first.checkout_id())
            .unwrap()
            .unwrap()
            .state(),
        CheckoutState::Claimed {
            dispatch: CheckoutDispatchState::GatewayRecorded,
            ..
        }
    ));
    let pending = reopened
        .pending_dispatch_after(None, ReconciliationPageLimit::new(10).unwrap())
        .unwrap();
    assert_eq!(pending.checkouts.len(), 1);
    assert_eq!(pending.checkouts[0].id(), second.checkout_id());
    assert!(
        reopened
            .get(TenantId::new([99; 32]), first.checkout_id())
            .unwrap()
            .is_none()
    );
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn network_binding_and_expired_claim_fail_closed() {
    let path = directory("network");
    let store =
        LmdbMerchantCheckoutStore::open(&path, NETWORK, MerchantCheckoutStoreOptions::default())
            .unwrap();
    let checkout = definition(1);
    store.create(checkout, 100).unwrap();
    assert_eq!(
        store.claim(
            TENANT,
            checkout.checkout_id(),
            AccountId::new([16; 32]),
            2,
            1_000,
        ),
        Err(MerchantCheckoutStoreError::Domain(CheckoutError::Expired))
    );
    drop(store);
    assert_eq!(
        LmdbMerchantCheckoutStore::open(
            &path,
            NetworkId::new([99; 32]),
            MerchantCheckoutStoreOptions::default(),
        )
        .unwrap_err(),
        MerchantCheckoutStoreError::WrongNetwork
    );
    fs::remove_dir_all(path).unwrap();
}
