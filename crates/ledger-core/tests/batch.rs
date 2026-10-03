use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement, Event,
    IdempotencyKey, Ledger, LedgerError, MAX_BATCH_ITEMS, NetworkId, TransferBatch, TransferItem,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const CAROL: AccountId = AccountId::new([8; 32]);
const NETWORK_TOKEN: AssetId = AssetId::new([11; 32]);
const KEY_ONE: IdempotencyKey = IdempotencyKey::new([21; 32]);
const KEY_TWO: IdempotencyKey = IdempotencyKey::new([22; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);

fn funded_ledger() -> Ledger {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: NETWORK_TOKEN,
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
    ledger.mint(ISSUER, NETWORK_TOKEN, ALICE, 10_000).unwrap();
    ledger
}

fn batch(items: Vec<TransferItem>) -> TransferBatch {
    TransferBatch {
        network: NETWORK,
        idempotency_key: KEY_ONE,
        asset: NETWORK_TOKEN,
        from: ALICE,
        items,
        fee: 7,
        nonce: 0,
        valid_until_height: 10_000,
    }
}

#[test]
fn batch_is_atomic_and_consumes_one_nonce() {
    let mut ledger = funded_ledger();
    let payments = vec![
        TransferItem {
            to: BOB,
            amount: 600,
        },
        TransferItem {
            to: CAROL,
            amount: 400,
        },
    ];

    let receipt = ledger
        .transfer_batch(ALICE, batch(payments.clone()))
        .unwrap();

    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 8_993);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 600);
    assert_eq!(ledger.balance(NETWORK_TOKEN, CAROL), 400);
    assert_eq!(ledger.balance(NETWORK_TOKEN, TREASURY), 7);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert!(
        ledger
            .audit_asset(NETWORK_TOKEN)
            .unwrap()
            .supply_matches_balances
    );
    assert_eq!(
        ledger.events().last(),
        Some(&Event::BatchTransferred {
            operation_id: receipt.operation_id,
            idempotency_key: KEY_ONE,
            asset: NETWORK_TOKEN,
            from: ALICE,
            fee_payer: ALICE,
            items: payments,
            fee: 7,
            nonce: 0,
        })
    );
}

#[test]
fn duplicate_recipients_are_aggregated_without_losing_value() {
    let mut ledger = funded_ledger();
    ledger
        .transfer_batch(
            ALICE,
            batch(vec![
                TransferItem {
                    to: BOB,
                    amount: 30,
                },
                TransferItem {
                    to: BOB,
                    amount: 20,
                },
            ]),
        )
        .unwrap();

    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 50);
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 9_943);
    assert!(
        ledger
            .audit_asset(NETWORK_TOKEN)
            .unwrap()
            .supply_matches_balances
    );
}

#[test]
fn one_invalid_recipient_rolls_back_the_whole_batch() {
    let mut ledger = funded_ledger();
    ledger
        .set_account_frozen(FREEZER, NETWORK_TOKEN, CAROL, true)
        .unwrap();
    let event_count = ledger.events().len();

    let result = ledger.transfer_batch(
        ALICE,
        batch(vec![
            TransferItem {
                to: BOB,
                amount: 100,
            },
            TransferItem {
                to: CAROL,
                amount: 200,
            },
        ]),
    );

    assert_eq!(result, Err(LedgerError::AccountFrozen));
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 10_000);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), 0);
    assert_eq!(ledger.balance(NETWORK_TOKEN, CAROL), 0);
    assert_eq!(ledger.balance(NETWORK_TOKEN, TREASURY), 0);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.events().len(), event_count);
}

#[test]
fn empty_and_oversized_batches_are_rejected() {
    let mut ledger = funded_ledger();
    assert_eq!(
        ledger.transfer_batch(ALICE, batch(Vec::new())),
        Err(LedgerError::EmptyBatch)
    );

    let items = vec![TransferItem { to: BOB, amount: 1 }; MAX_BATCH_ITEMS + 1];
    assert_eq!(
        ledger.transfer_batch(ALICE, batch(items)),
        Err(LedgerError::BatchTooLarge)
    );
    assert_eq!(ledger.nonce(ALICE), 0);
}

#[test]
fn arithmetic_overflow_does_not_mutate_state() {
    let mut ledger = funded_ledger();
    let event_count = ledger.events().len();
    let result = ledger.transfer_batch(
        ALICE,
        batch(vec![
            TransferItem {
                to: BOB,
                amount: u128::MAX,
            },
            TransferItem {
                to: CAROL,
                amount: 1,
            },
        ]),
    );

    assert_eq!(result, Err(LedgerError::ArithmeticOverflow));
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 10_000);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.events().len(), event_count);
}

#[test]
fn batch_requires_sender_authorization_and_rejects_replay() {
    let mut ledger = funded_ledger();
    let payment = batch(vec![TransferItem {
        to: BOB,
        amount: 10,
    }]);

    assert_eq!(
        ledger.transfer_batch(BOB, payment.clone()),
        Err(LedgerError::Unauthorized)
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), 10_000);
    assert_eq!(ledger.nonce(ALICE), 0);

    ledger.transfer_batch(ALICE, payment.clone()).unwrap();
    let sender_balance = ledger.balance(NETWORK_TOKEN, ALICE);
    let recipient_balance = ledger.balance(NETWORK_TOKEN, BOB);
    assert_eq!(
        ledger.transfer_batch(ALICE, payment.clone()),
        Err(LedgerError::NonceMismatch {
            expected: 1,
            actual: 0,
        })
    );
    let stale_nonce_with_fresh_key = TransferBatch {
        idempotency_key: KEY_TWO,
        ..payment
    };
    assert_eq!(
        ledger.transfer_batch(ALICE, stale_nonce_with_fresh_key),
        Err(LedgerError::NonceMismatch {
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(ledger.balance(NETWORK_TOKEN, ALICE), sender_balance);
    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), recipient_balance);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert_eq!(ledger.next_operation_index(), 1);
}

#[test]
fn maximum_batch_size_is_accepted() {
    let mut ledger = funded_ledger();
    let items = vec![TransferItem { to: BOB, amount: 1 }; MAX_BATCH_ITEMS];

    ledger.transfer_batch(ALICE, batch(items)).unwrap();

    assert_eq!(ledger.balance(NETWORK_TOKEN, BOB), MAX_BATCH_ITEMS as u128);
    assert_eq!(ledger.nonce(ALICE), 1);
    assert!(
        ledger
            .audit_asset(NETWORK_TOKEN)
            .unwrap()
            .supply_matches_balances
    );
}
