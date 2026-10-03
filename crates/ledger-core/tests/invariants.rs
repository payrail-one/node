use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement,
    IdempotencyKey, Ledger, NetworkId, Transfer,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ACCOUNTS: [AccountId; 4] = [
    AccountId::new([6; 32]),
    AccountId::new([7; 32]),
    AccountId::new([8; 32]),
    AccountId::new([9; 32]),
];
const TOKEN: AssetId = AssetId::new([11; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);

fn operation_key(sequence: u64) -> IdempotencyKey {
    let mut value = [0_u8; 32];
    value[..8].copy_from_slice(&sequence.to_be_bytes());
    IdempotencyKey::new(value)
}

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
                max_supply: Some(40_000),
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    for account in ACCOUNTS {
        ledger.mint(ISSUER, TOKEN, account, 10_000).unwrap();
    }
    ledger
}

#[test]
fn long_deterministic_payment_sequence_preserves_all_invariants() {
    let mut ledger = funded_ledger();
    let mut nonces = [0_u64; ACCOUNTS.len()];

    for sequence in 0_u64..400 {
        let sender_index = usize::try_from(sequence % 4).unwrap();
        let recipient_index = (sender_index + 1) % ACCOUNTS.len();
        let amount = u128::from((sequence % 17) + 1);
        let fee = u128::from(sequence % 3);
        let sender = ACCOUNTS[sender_index];

        let receipt = ledger
            .transfer(
                sender,
                Transfer {
                    network: NETWORK,
                    idempotency_key: operation_key(sequence),
                    asset: TOKEN,
                    from: sender,
                    to: ACCOUNTS[recipient_index],
                    amount,
                    fee,
                    nonce: nonces[sender_index],
                    valid_until_height: 10_000,
                },
            )
            .unwrap();
        nonces[sender_index] += 1;

        assert_eq!(receipt.nonce + 1, nonces[sender_index]);
        assert!(ledger.audit_asset(TOKEN).unwrap().supply_matches_balances);
    }

    let user_total = ACCOUNTS
        .iter()
        .map(|account| ledger.balance(TOKEN, *account))
        .sum::<u128>();
    assert_eq!(user_total + ledger.balance(TOKEN, TREASURY), 40_000);
    assert_eq!(
        Ledger::from_snapshot(NETWORK, ledger.snapshot())
            .unwrap()
            .snapshot(),
        ledger.snapshot()
    );
}
