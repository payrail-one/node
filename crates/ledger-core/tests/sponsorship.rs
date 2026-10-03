use ledger_core::{
    AccountId, AccountStatus, AssetClass, AssetDefinition, AssetId, AssetStatus,
    BackingRequirement, IdempotencyKey, Ledger, LedgerError, NetworkId, OperationKind, Transfer,
    TransferBatch, TransferItem,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const SPONSOR: AccountId = AccountId::new([8; 32]);
const TOKEN: AssetId = AssetId::new([11; 32]);
const KEY_ONE: IdempotencyKey = IdempotencyKey::new([21; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);

fn funded_ledger() -> Ledger {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: TOKEN,
                symbol: "PAY".to_owned(),
                decimals: 6,
                class: AssetClass::NetworkNative,
                status: AssetStatus::Active,
                issuer: ISSUER,
                backing_authority: BACKING,
                freeze_authority: FREEZER,
                treasury: TREASURY,
                max_supply: None,
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, TOKEN, ALICE, 1_000).unwrap();
    ledger.mint(ISSUER, TOKEN, SPONSOR, 100).unwrap();
    ledger
}

fn payment(fee: u128) -> Transfer {
    Transfer {
        network: NETWORK,
        idempotency_key: KEY_ONE,
        asset: TOKEN,
        from: ALICE,
        to: BOB,
        amount: 100,
        fee,
        nonce: 0,
        valid_until_height: 10_000,
    }
}

#[test]
fn separately_authorized_sponsor_pays_transfer_fee() {
    let mut ledger = funded_ledger();
    let receipt = ledger
        .transfer_sponsored(ALICE, SPONSOR, SPONSOR, payment(5))
        .unwrap();

    assert_eq!(receipt.kind, OperationKind::SponsoredTransfer);
    assert_eq!(ledger.balance(TOKEN, ALICE), 900);
    assert_eq!(ledger.balance(TOKEN, SPONSOR), 95);
    assert_eq!(ledger.balance(TOKEN, BOB), 100);
    assert_eq!(ledger.balance(TOKEN, TREASURY), 5);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert_eq!(ledger.nonce(SPONSOR), 0);
    assert_eq!(receipt.operation_index, 0);
    assert_eq!(ledger.next_operation_index(), 1);
    assert!(ledger.audit_asset(TOKEN).unwrap().supply_matches_balances);
}

#[test]
fn sponsorship_requires_explicit_payer_authorization() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.transfer_sponsored(ALICE, BOB, SPONSOR, payment(5)),
        Err(LedgerError::Unauthorized)
    );

    assert_eq!(ledger.balance(TOKEN, ALICE), 1_000);
    assert_eq!(ledger.balance(TOKEN, SPONSOR), 100);
    assert_eq!(ledger.balance(TOKEN, BOB), 0);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.next_operation_index(), 0);
}

#[test]
fn sponsor_failure_rolls_back_the_entire_payment() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.transfer_sponsored(ALICE, SPONSOR, SPONSOR, payment(101)),
        Err(LedgerError::InsufficientBalance)
    );

    assert_eq!(ledger.balance(TOKEN, ALICE), 1_000);
    assert_eq!(ledger.balance(TOKEN, SPONSOR), 100);
    assert_eq!(ledger.balance(TOKEN, BOB), 0);
    assert_eq!(ledger.balance(TOKEN, TREASURY), 0);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.next_operation_index(), 0);
}

#[test]
fn restricted_account_cannot_sponsor_fees() {
    let mut ledger = funded_ledger();
    ledger
        .set_account_status(FREEZER, TOKEN, SPONSOR, AccountStatus::ReceiveOnly)
        .unwrap();

    assert_eq!(
        ledger.transfer_sponsored(ALICE, SPONSOR, SPONSOR, payment(5)),
        Err(LedgerError::AccountCannotSend)
    );
    assert_eq!(ledger.balance(TOKEN, ALICE), 1_000);
    assert_eq!(ledger.nonce(ALICE), 0);
}

#[test]
fn zero_fee_or_sender_as_sponsor_is_rejected() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.transfer_sponsored(ALICE, SPONSOR, SPONSOR, payment(0)),
        Err(LedgerError::FeeSponsorshipNotApplicable)
    );
    assert_eq!(
        ledger.transfer_sponsored(ALICE, ALICE, ALICE, payment(5)),
        Err(LedgerError::FeeSponsorshipNotApplicable)
    );
}

#[test]
fn sponsor_can_pay_one_fee_for_atomic_batch() {
    let mut ledger = funded_ledger();
    let receipt = ledger
        .transfer_batch_sponsored(
            ALICE,
            SPONSOR,
            SPONSOR,
            TransferBatch {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: TOKEN,
                from: ALICE,
                items: vec![
                    TransferItem {
                        to: BOB,
                        amount: 60,
                    },
                    TransferItem {
                        to: TREASURY,
                        amount: 40,
                    },
                ],
                fee: 3,
                nonce: 0,
                valid_until_height: 10_000,
            },
        )
        .unwrap();

    assert_eq!(receipt.kind, OperationKind::SponsoredBatchTransfer);
    assert_eq!(ledger.balance(TOKEN, ALICE), 900);
    assert_eq!(ledger.balance(TOKEN, SPONSOR), 97);
    assert_eq!(ledger.balance(TOKEN, BOB), 60);
    assert_eq!(ledger.balance(TOKEN, TREASURY), 43);
    assert!(ledger.audit_asset(TOKEN).unwrap().supply_matches_balances);
}
