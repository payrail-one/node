use ledger_core::{
    AccountId, AccountStatus, AssetClass, AssetDefinition, AssetId, AssetStatus,
    BackingRequirement, Event, IdempotencyKey, Ledger, LedgerError, NetworkId, Transfer,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const TOKEN: AssetId = AssetId::new([11; 32]);
const KEY_ONE: IdempotencyKey = IdempotencyKey::new([21; 32]);
const KEY_TWO: IdempotencyKey = IdempotencyKey::new([22; 32]);
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
    ledger
}

fn transfer(from: AccountId, to: AccountId, key: IdempotencyKey) -> Transfer {
    Transfer {
        network: NETWORK,
        idempotency_key: key,
        asset: TOKEN,
        from,
        to,
        amount: 100,
        fee: 0,
        nonce: 0,
        valid_until_height: 10_000,
    }
}

#[test]
fn only_freeze_authority_can_change_status() {
    let mut ledger = funded_ledger();
    assert_eq!(ledger.account_status(TOKEN, ALICE), AccountStatus::Active);

    assert_eq!(
        ledger.set_account_status(ALICE, TOKEN, BOB, AccountStatus::Frozen),
        Err(LedgerError::Unauthorized)
    );
    ledger
        .set_account_status(FREEZER, TOKEN, BOB, AccountStatus::ReceiveOnly)
        .unwrap();

    assert_eq!(
        ledger.account_status(TOKEN, BOB),
        AccountStatus::ReceiveOnly
    );
    assert_eq!(
        ledger.events().last(),
        Some(&Event::AccountStatusChanged {
            asset: TOKEN,
            account: BOB,
            status: AccountStatus::ReceiveOnly,
        })
    );
}

#[test]
fn receive_only_account_can_receive_but_cannot_send() {
    let mut ledger = funded_ledger();
    ledger
        .set_account_status(FREEZER, TOKEN, BOB, AccountStatus::ReceiveOnly)
        .unwrap();

    ledger
        .transfer(ALICE, transfer(ALICE, BOB, KEY_ONE))
        .unwrap();
    assert_eq!(ledger.balance(TOKEN, BOB), 100);
    assert_eq!(
        ledger.transfer(BOB, transfer(BOB, ALICE, KEY_TWO)),
        Err(LedgerError::AccountCannotSend)
    );
    assert_eq!(ledger.balance(TOKEN, BOB), 100);
    assert_eq!(ledger.nonce(BOB), 0);
    assert_eq!(ledger.next_operation_index(), 1);
}

#[test]
fn send_only_account_can_send_but_cannot_receive_or_be_minted_to() {
    let mut ledger = funded_ledger();
    ledger.mint(ISSUER, TOKEN, BOB, 200).unwrap();
    ledger
        .set_account_status(FREEZER, TOKEN, BOB, AccountStatus::SendOnly)
        .unwrap();

    assert_eq!(
        ledger.transfer(ALICE, transfer(ALICE, BOB, KEY_ONE)),
        Err(LedgerError::AccountCannotReceive)
    );
    assert_eq!(
        ledger.mint(ISSUER, TOKEN, BOB, 1),
        Err(LedgerError::AccountCannotReceive)
    );
    ledger.transfer(BOB, transfer(BOB, ALICE, KEY_TWO)).unwrap();

    assert_eq!(ledger.balance(TOKEN, BOB), 100);
    assert_eq!(ledger.balance(TOKEN, ALICE), 1_100);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.nonce(BOB), 1);
}

#[test]
fn restricted_treasury_cannot_receive_fees() {
    let mut ledger = funded_ledger();
    ledger
        .set_account_status(FREEZER, TOKEN, TREASURY, AccountStatus::Frozen)
        .unwrap();
    let mut payment = transfer(ALICE, BOB, KEY_ONE);
    payment.fee = 1;

    assert_eq!(
        ledger.transfer(ALICE, payment),
        Err(LedgerError::AccountFrozen)
    );
    assert_eq!(ledger.balance(TOKEN, ALICE), 1_000);
    assert_eq!(ledger.nonce(ALICE), 0);
    assert_eq!(ledger.next_operation_index(), 0);

    payment.fee = 0;
    ledger.transfer(ALICE, payment).unwrap();
    assert_eq!(ledger.balance(TOKEN, BOB), 100);
}
