use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement,
    IdempotencyKey, Ledger, LedgerError, NetworkId, Transfer,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const OTHER_ISSUER: AccountId = AccountId::new([8; 32]);
const NETWORK_TOKEN: AssetId = AssetId::new([11; 32]);
const EXTERNAL_ASSET: AssetId = AssetId::new([12; 32]);
const SOVEREIGN_ASSET: AssetId = AssetId::new([13; 32]);
const KEY_ONE: IdempotencyKey = IdempotencyKey::new([21; 32]);
const KEY_TWO: IdempotencyKey = IdempotencyKey::new([22; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([43; 32]);

fn definition(id: AssetId, symbol: &str, class: AssetClass) -> AssetDefinition {
    AssetDefinition {
        id,
        symbol: symbol.to_owned(),
        decimals: 6,
        class,
        status: AssetStatus::Active,
        issuer: ISSUER,
        backing_authority: BACKING,
        freeze_authority: FREEZER,
        treasury: TREASURY,
        max_supply: Some(1_000_000),
        backing_requirement: BackingRequirement::None,
    }
}

fn funded_ledger() -> Ledger {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            definition(NETWORK_TOKEN, "PAY", AssetClass::NetworkNative),
        )
        .unwrap();
    ledger.mint(ISSUER, NETWORK_TOKEN, ALICE, 1_000).unwrap();
    ledger
}

#[test]
fn transfer_is_atomic_and_conserves_supply() {
    let mut ledger = funded_ledger();
    let receipt = ledger
        .transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: NETWORK_TOKEN,
                from: ALICE,
                to: BOB,
                amount: 250,
                fee: 5,
                nonce: 0,
                valid_until_height: 10_000,
            },
        )
        .unwrap();

    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 745);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 250);
    assert_eq!(ledger.balance(NETWORK_TOKEN, TREASURY), 5);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert_eq!(receipt.operation_index, 0);
    assert_eq!(ledger.next_operation_index(), 1);
    let audit = ledger.audit_asset(NETWORK_TOKEN).unwrap();
    assert!(audit.supply_matches_balances);
    assert_eq!(audit.declared_supply, 1_000);
}

#[test]
fn replayed_nonce_is_rejected_without_state_changes() {
    let mut ledger = funded_ledger();
    let transfer = Transfer {
        network: NETWORK,
        idempotency_key: KEY_ONE,
        asset: NETWORK_TOKEN,
        from: ALICE,
        to: BOB,
        amount: 100,
        fee: 1,
        nonce: 0,
        valid_until_height: 10_000,
    };
    ledger.transfer(ALICE, transfer).unwrap();
    let before = ledger.balance(NETWORK_TOKEN, ALICE);

    assert_eq!(
        ledger.transfer(ALICE, transfer),
        Err(LedgerError::NonceMismatch {
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), before);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert_eq!(ledger.next_operation_index(), 1);

    let stale_nonce_with_fresh_key = Transfer {
        idempotency_key: KEY_TWO,
        ..transfer
    };
    assert_eq!(
        ledger.transfer(ALICE, stale_nonce_with_fresh_key),
        Err(LedgerError::NonceMismatch {
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), before);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert_eq!(ledger.next_operation_index(), 1);
}

#[test]
fn nonce_is_the_consensus_replay_key_not_client_correlation_data() {
    let mut ledger = funded_ledger();
    let first = Transfer {
        network: NETWORK,
        idempotency_key: KEY_ONE,
        asset: NETWORK_TOKEN,
        from: ALICE,
        to: BOB,
        amount: 10,
        fee: 0,
        nonce: 0,
        valid_until_height: 10_000,
    };
    let first_receipt = ledger.transfer(ALICE, first).unwrap();
    let second_receipt = ledger
        .transfer(ALICE, Transfer { nonce: 1, ..first })
        .unwrap();

    assert_eq!(first_receipt.operation_index, 0);
    assert_eq!(second_receipt.operation_index, 1);
    assert_eq!(ledger.nonce(ALICE), 2);
    assert_eq!(ledger.next_operation_index(), 2);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 20);
}

#[test]
fn insufficient_transfer_does_not_charge_fee_or_nonce() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: NETWORK_TOKEN,
                from: ALICE,
                to: BOB,
                amount: 1_000,
                fee: 1,
                nonce: 0,
                valid_until_height: 10_000,
            }
        ),
        Err(LedgerError::InsufficientBalance)
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 1_000);
    assert_eq!(ledger.balance(NETWORK_TOKEN, TREASURY), 0);
    assert_eq!(ledger.nonce(ALICE), 0);
}

#[test]
fn external_asset_cannot_exceed_verified_backing() {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    let mut external = definition(EXTERNAL_ASSET, "BTC", AssetClass::ExternalRepresentation);
    external.decimals = 8;
    external.backing_requirement = BackingRequirement::VerifiedOneToOne;
    ledger.register_asset(REGISTRY, external).unwrap();
    ledger
        .observe_backing(BACKING, EXTERNAL_ASSET, 500)
        .unwrap();

    ledger.mint(ISSUER, EXTERNAL_ASSET, ALICE, 500).unwrap();
    assert_eq!(
        ledger.mint(ISSUER, EXTERNAL_ASSET, ALICE, 1),
        Err(LedgerError::BackingExceeded)
    );
    assert!(
        ledger
            .audit_asset(EXTERNAL_ASSET)
            .unwrap()
            .backing_covers_supply
    );
}

#[test]
fn backing_deficit_is_visible_and_suspends_asset() {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    let mut external = definition(EXTERNAL_ASSET, "USDT", AssetClass::ExternalRepresentation);
    external.backing_requirement = BackingRequirement::VerifiedOneToOne;
    ledger.register_asset(REGISTRY, external).unwrap();
    ledger
        .observe_backing(BACKING, EXTERNAL_ASSET, 100)
        .unwrap();
    ledger.mint(ISSUER, EXTERNAL_ASSET, ALICE, 100).unwrap();

    ledger.observe_backing(BACKING, EXTERNAL_ASSET, 90).unwrap();
    let audit = ledger.audit_asset(EXTERNAL_ASSET).unwrap();
    assert!(!audit.backing_covers_supply);
    assert_eq!(audit.status, AssetStatus::Suspended);
    assert_eq!(
        ledger.set_status(REGISTRY, EXTERNAL_ASSET, AssetStatus::Active),
        Err(LedgerError::BackingDeficit)
    );
    assert_eq!(
        ledger.transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: EXTERNAL_ASSET,
                from: ALICE,
                to: BOB,
                amount: 1,
                fee: 0,
                nonce: 0,
                valid_until_height: 10_000,
            }
        ),
        Err(LedgerError::AssetNotTransferable)
    );
}

#[test]
fn issuance_authority_is_scoped_per_asset() {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    let mut sovereign = definition(SOVEREIGN_ASSET, "CDF", AssetClass::Sovereign);
    sovereign.issuer = OTHER_ISSUER;
    ledger.register_asset(REGISTRY, sovereign).unwrap();

    assert_eq!(
        ledger.mint(ISSUER, SOVEREIGN_ASSET, ALICE, 100),
        Err(LedgerError::Unauthorized)
    );
    ledger
        .mint(OTHER_ISSUER, SOVEREIGN_ASSET, ALICE, 100)
        .unwrap();
}

#[test]
fn frozen_account_cannot_send_or_receive() {
    let mut ledger = funded_ledger();
    ledger
        .set_account_frozen(FREEZER, NETWORK_TOKEN, BOB, true)
        .unwrap();

    assert_eq!(
        ledger.transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: NETWORK_TOKEN,
                from: ALICE,
                to: BOB,
                amount: 10,
                fee: 0,
                nonce: 0,
                valid_until_height: 10_000,
            }
        ),
        Err(LedgerError::AccountFrozen)
    );
}

#[test]
fn supply_cap_and_zero_amount_are_enforced() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.mint(ISSUER, NETWORK_TOKEN, ALICE, 0),
        Err(LedgerError::InvalidAmount)
    );
    assert_eq!(
        ledger.mint(ISSUER, NETWORK_TOKEN, ALICE, 1_000_000),
        Err(LedgerError::SupplyCapExceeded)
    );
}

#[test]
fn burn_reduces_balance_and_declared_supply_together() {
    let mut ledger = funded_ledger();
    ledger.burn(ISSUER, NETWORK_TOKEN, ALICE, 200).unwrap();

    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 800);
    let audit = ledger.audit_asset(NETWORK_TOKEN).unwrap();
    assert_eq!(audit.declared_supply, 800);
    assert_eq!(audit.summed_balances, 800);
    assert!(audit.supply_matches_balances);
}

#[test]
fn only_registry_authority_can_register_or_change_status() {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    let asset = definition(NETWORK_TOKEN, "PAY", AssetClass::NetworkNative);
    assert_eq!(
        ledger.register_asset(ALICE, asset.clone()),
        Err(LedgerError::Unauthorized)
    );
    ledger.register_asset(REGISTRY, asset).unwrap();
    assert_eq!(
        ledger.set_status(ALICE, NETWORK_TOKEN, AssetStatus::Suspended),
        Err(LedgerError::Unauthorized)
    );
}

#[test]
fn transaction_from_another_network_is_rejected_before_state_changes() {
    let mut ledger = funded_ledger();
    let mut transfer = Transfer {
        network: OTHER_NETWORK,
        idempotency_key: KEY_ONE,
        asset: NETWORK_TOKEN,
        from: ALICE,
        to: BOB,
        amount: 10,
        fee: 1,
        nonce: 0,
        valid_until_height: 10_000,
    };

    assert_eq!(
        ledger.transfer(ALICE, transfer),
        Err(LedgerError::WrongNetwork)
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 1_000);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 0);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.next_operation_index(), 0);

    transfer.network = NETWORK;
    ledger.transfer(ALICE, transfer).unwrap();
    assert_eq!(ledger.nonce(ALICE), 1);
}
