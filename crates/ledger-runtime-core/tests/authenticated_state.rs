use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement,
    IdempotencyKey, Ledger, LedgerError, NetworkId, Transfer,
};
use ledger_runtime_core::{
    LedgerAuthenticatedState, LedgerStateNamespace, RuntimeError, authenticated_state_root,
};
use state_sync_core::StateRoot;

const NETWORK: NetworkId = NetworkId::new([81; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([82; 32]);
const TOKEN: AssetId = AssetId::new([83; 32]);
const REGISTRY: AccountId = AccountId::new([84; 32]);
const ISSUER: AccountId = AccountId::new([85; 32]);
const BACKING: AccountId = AccountId::new([86; 32]);
const FREEZER: AccountId = AccountId::new([87; 32]);
const TREASURY: AccountId = AccountId::new([88; 32]);
const SENDER: AccountId = AccountId::new([89; 32]);
const RECIPIENT: AccountId = AccountId::new([90; 32]);

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
                max_supply: Some(1_000_000),
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, TOKEN, SENDER, 10_000).unwrap();
    ledger
}

fn transfer(ledger: &mut Ledger, nonce: u64, marker: u8, amount: u128) {
    ledger
        .transfer(
            SENDER,
            Transfer {
                network: NETWORK,
                idempotency_key: IdempotencyKey::new([marker; 32]),
                asset: TOKEN,
                from: SENDER,
                to: RECIPIENT,
                amount,
                fee: 5,
                nonce,
                valid_until_height: 10_000,
            },
        )
        .unwrap();
}

#[test]
fn incremental_rows_match_rebuild_and_retain_height_proofs() {
    let mut ledger = funded_ledger();
    let initial = ledger.snapshot();
    let mut state = LedgerAuthenticatedState::create(NETWORK, 100, &initial).unwrap();
    let initial_root = state.root();
    transfer(&mut ledger, 0, 91, 1_250);
    let updated = ledger.snapshot();

    let updated_root = state.apply(101, &updated).unwrap();

    assert_eq!(updated_root, authenticated_state_root(&updated).unwrap());
    assert_eq!(state.root_at(100).unwrap(), initial_root);
    assert_eq!(state.root_at(101).unwrap(), updated_root);
    assert_eq!(state.base_height(), 100);
    assert_eq!(state.latest_height(), 101);

    let balance_key = [TOKEN.as_bytes().as_slice(), SENDER.as_bytes().as_slice()].concat();
    let old_proof = state
        .prove(100, LedgerStateNamespace::Balance, &balance_key)
        .unwrap();
    let new_proof = state
        .prove(101, LedgerStateNamespace::Balance, &balance_key)
        .unwrap();
    assert_eq!(
        old_proof.value(),
        Some(10_000_u128.to_be_bytes().as_slice())
    );
    assert_eq!(new_proof.value(), Some(8_745_u128.to_be_bytes().as_slice()));
    assert!(old_proof.verifies(initial_root));
    assert!(new_proof.verifies(updated_root));
    assert!(!old_proof.verifies(updated_root));
}

#[test]
fn prepared_updates_are_inert_and_competing_commit_is_rejected() {
    let mut first_ledger = funded_ledger();
    let initial = first_ledger.snapshot();
    let mut state = LedgerAuthenticatedState::create(NETWORK, 100, &initial).unwrap();
    let initial_root = state.root();

    transfer(&mut first_ledger, 0, 94, 1_000);
    let winner = state.prepare(101, &first_ledger.snapshot()).unwrap();

    let mut second_ledger = funded_ledger();
    transfer(&mut second_ledger, 0, 95, 2_000);
    let loser = state.prepare(101, &second_ledger.snapshot()).unwrap();

    assert_eq!(winner.height(), 101);
    assert_eq!(state.root(), initial_root);
    assert_eq!(state.latest_height(), 100);

    let winner_root = state.commit(winner).unwrap();
    assert_eq!(state.root(), winner_root);
    assert_eq!(state.latest_height(), 101);
    assert_eq!(state.commit(loser), Err(RuntimeError::InvalidStateHeight));
    assert_eq!(state.root(), winner_root);
    assert_eq!(state.latest_height(), 101);
}

#[test]
fn normalized_ledger_root_is_stable_and_row_deletion_is_incremental() {
    let mut ledger = funded_ledger();
    let initial = ledger.snapshot();
    let mut state = LedgerAuthenticatedState::create(NETWORK, 700, &initial).unwrap();
    assert_eq!(
        state.root(),
        StateRoot::new([
            83, 172, 78, 237, 15, 41, 130, 191, 38, 95, 112, 19, 10, 235, 126, 119, 125, 244, 200,
            152, 218, 87, 118, 121, 58, 50, 153, 78, 88, 224, 1, 143,
        ])
    );

    transfer(&mut ledger, 0, 93, 9_995);
    let emptied = ledger.snapshot();
    let updated_root = state.apply(701, &emptied).unwrap();
    assert_eq!(updated_root, authenticated_state_root(&emptied).unwrap());

    let sender_balance = [TOKEN.as_bytes().as_slice(), SENDER.as_bytes().as_slice()].concat();
    let proof = state
        .prove(701, LedgerStateNamespace::Balance, &sender_balance)
        .unwrap();
    assert_eq!(proof.value(), None);
    assert!(proof.verifies(updated_root));
}

#[test]
fn missing_row_has_a_valid_non_membership_proof() {
    let snapshot = funded_ledger().snapshot();
    let state = LedgerAuthenticatedState::create(NETWORK, 500, &snapshot).unwrap();
    let missing_key = [TOKEN.as_bytes().as_slice(), RECIPIENT.as_bytes().as_slice()].concat();

    let proof = state
        .prove(500, LedgerStateNamespace::Balance, &missing_key)
        .unwrap();

    assert_eq!(proof.value(), None);
    assert!(proof.verifies(state.root()));
    assert_eq!(proof.namespace(), LedgerStateNamespace::Balance);
    assert_eq!(proof.key(), missing_key);
}

#[test]
fn invalid_height_network_and_identity_changes_are_atomic() {
    let snapshot = funded_ledger().snapshot();
    let mut state = LedgerAuthenticatedState::create(NETWORK, 100, &snapshot).unwrap();
    let root = state.root();
    assert_eq!(
        state.apply(102, &snapshot),
        Err(RuntimeError::InvalidStateHeight)
    );

    let mut wrong_network = snapshot.clone();
    wrong_network.network = OTHER_NETWORK;
    assert!(matches!(
        LedgerAuthenticatedState::create(NETWORK, 100, &wrong_network),
        Err(RuntimeError::InvalidSnapshot(LedgerError::WrongNetwork))
    ));
    assert_eq!(
        state.apply(101, &wrong_network),
        Err(RuntimeError::StateIdentityChanged)
    );

    let mut changed_registry = snapshot;
    changed_registry.registry_authority = AccountId::new([92; 32]);
    assert_eq!(
        state.apply(101, &changed_registry),
        Err(RuntimeError::StateIdentityChanged)
    );
    assert_eq!(state.root(), root);
    assert_eq!(state.latest_height(), 100);
    assert_eq!(state.root_at(99), Err(RuntimeError::InvalidStateHeight));
    assert_eq!(state.root_at(101), Err(RuntimeError::InvalidStateHeight));
}

#[test]
fn proof_is_bound_to_namespace_value_and_root() {
    let snapshot = funded_ledger().snapshot();
    let state = LedgerAuthenticatedState::create(NETWORK, 0, &snapshot).unwrap();
    let proof = state
        .prove(0, LedgerStateNamespace::Asset, TOKEN.as_bytes())
        .unwrap();

    assert!(proof.value().is_some());
    assert!(proof.verifies(state.root()));
    assert!(!proof.verifies(StateRoot::new([0; 32])));
    let wrong_namespace = state
        .prove(0, LedgerStateNamespace::Nonce, TOKEN.as_bytes())
        .unwrap();
    assert_eq!(wrong_namespace.value(), None);
    assert!(wrong_namespace.verifies(state.root()));
}
