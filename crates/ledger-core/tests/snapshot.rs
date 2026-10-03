use ledger_core::{
    AccountId, AccountStatus, AssetClass, AssetDefinition, AssetId, AssetStatus,
    BackingRequirement, IdempotencyKey, Ledger, LedgerError, NetworkId, Transfer, TransferBatch,
    TransferItem,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const CAROL: AccountId = AccountId::new([8; 32]);
const TOKEN: AssetId = AssetId::new([11; 32]);
const UNKNOWN_ASSET: AssetId = AssetId::new([99; 32]);
const KEY_ONE: IdempotencyKey = IdempotencyKey::new([21; 32]);
const KEY_TWO: IdempotencyKey = IdempotencyKey::new([22; 32]);
const KEY_THREE: IdempotencyKey = IdempotencyKey::new([23; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([43; 32]);

fn populated_ledger() -> Ledger {
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
                max_supply: Some(10_000),
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, TOKEN, ALICE, 1_000).unwrap();
    ledger
        .transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_ONE,
                asset: TOKEN,
                from: ALICE,
                to: BOB,
                amount: 100,
                fee: 1,
                nonce: 0,
                valid_until_height: 10_000,
            },
        )
        .unwrap();
    ledger
        .transfer_batch(
            ALICE,
            TransferBatch {
                network: NETWORK,
                idempotency_key: KEY_TWO,
                asset: TOKEN,
                from: ALICE,
                items: vec![
                    TransferItem {
                        to: BOB,
                        amount: 20,
                    },
                    TransferItem {
                        to: CAROL,
                        amount: 30,
                    },
                ],
                fee: 2,
                nonce: 1,
                valid_until_height: 10_000,
            },
        )
        .unwrap();
    ledger
        .set_account_status(FREEZER, TOKEN, CAROL, AccountStatus::ReceiveOnly)
        .unwrap();
    ledger
}

#[test]
fn snapshot_round_trip_preserves_bounded_consensus_state() {
    let ledger = populated_ledger();
    let snapshot = ledger.snapshot();
    let restored = Ledger::from_snapshot(NETWORK, snapshot.clone()).unwrap();

    assert_eq!(restored.snapshot(), snapshot);
    assert!(restored.events().is_empty());
    assert_eq!(restored.balance(TOKEN, ALICE), 847);
    assert_eq!(restored.balance(TOKEN, BOB), 120);
    assert_eq!(restored.balance(TOKEN, CAROL), 30);
    assert_eq!(restored.balance(TOKEN, TREASURY), 3);
    assert_eq!(restored.nonce(ALICE), 2);
    assert_eq!(restored.next_operation_index(), 2);
    assert_eq!(
        restored.account_status(TOKEN, CAROL),
        AccountStatus::ReceiveOnly
    );
}

#[test]
fn restored_ledger_continues_the_global_operation_sequence() {
    let mut restored = Ledger::from_snapshot(NETWORK, populated_ledger().snapshot()).unwrap();

    let receipt = restored
        .transfer(
            ALICE,
            Transfer {
                network: NETWORK,
                idempotency_key: KEY_THREE,
                asset: TOKEN,
                from: ALICE,
                to: BOB,
                amount: 10,
                fee: 1,
                nonce: 2,
                valid_until_height: 10_000,
            },
        )
        .unwrap();

    assert_eq!(receipt.operation_index, 2);
    Ledger::from_snapshot(NETWORK, restored.snapshot()).unwrap();
}

#[test]
fn exhausted_operation_sequence_rejects_without_mutation() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.nonces[0].nonce = u64::MAX;
    snapshot.next_operation_index = u64::MAX;
    let mut ledger = Ledger::from_snapshot(NETWORK, snapshot).unwrap();
    let before = ledger.snapshot();

    let result = ledger.transfer(
        ALICE,
        Transfer {
            network: NETWORK,
            idempotency_key: KEY_THREE,
            asset: TOKEN,
            from: ALICE,
            to: BOB,
            amount: 10,
            fee: 1,
            nonce: u64::MAX,
            valid_until_height: 10_000,
        },
    );

    assert_eq!(result, Err(LedgerError::ArithmeticOverflow));
    assert_eq!(ledger.snapshot(), before);
    assert!(ledger.events().is_empty());
}

#[test]
fn independently_built_equal_states_have_equal_snapshots() {
    assert_eq!(populated_ledger().snapshot(), populated_ledger().snapshot());
}

#[test]
fn snapshot_rejects_noncanonical_order() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.balances.swap(0, 1);

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::NonCanonicalSnapshot
    );
}

#[test]
fn snapshot_rejects_supply_mismatch_and_zero_balances() {
    let mut supply_mismatch = populated_ledger().snapshot();
    supply_mismatch.assets[0].supply += 1;
    assert_eq!(
        Ledger::from_snapshot(NETWORK, supply_mismatch).unwrap_err(),
        LedgerError::SnapshotSupplyMismatch
    );

    let mut zero_balance = populated_ledger().snapshot();
    zero_balance.balances[0].amount = 0;
    assert_eq!(
        Ledger::from_snapshot(NETWORK, zero_balance).unwrap_err(),
        LedgerError::SnapshotPolicyViolation
    );
}

#[test]
fn snapshot_rejects_unknown_asset_references() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.account_statuses[0].asset = UNKNOWN_ASSET;

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::SnapshotUnknownAsset
    );
}

#[test]
fn snapshot_rejects_broken_operation_sequence() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.next_operation_index = 1;

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::SnapshotOperationMismatch
    );
}

#[test]
fn snapshot_rejects_operation_sequence_above_nonce_sum() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.next_operation_index = 3;

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::SnapshotOperationMismatch
    );
}

#[test]
fn snapshot_rejects_zero_persisted_nonce() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.nonces[0].nonce = 0;

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::SnapshotOperationMismatch
    );
}

#[test]
fn snapshot_rejects_redundant_active_account_status() {
    let mut snapshot = populated_ledger().snapshot();
    snapshot.account_statuses[0].status = AccountStatus::Active;

    assert_eq!(
        Ledger::from_snapshot(NETWORK, snapshot).unwrap_err(),
        LedgerError::SnapshotPolicyViolation
    );
}

#[test]
fn snapshot_cannot_be_restored_into_another_network() {
    let snapshot = populated_ledger().snapshot();

    assert_eq!(
        Ledger::from_snapshot(OTHER_NETWORK, snapshot).unwrap_err(),
        LedgerError::WrongNetwork
    );
}
