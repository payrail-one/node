use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, NetworkId, SignatureBytes,
    SignedOperation, Transfer,
};
use ledger_runtime_core::{
    LedgerBlockCodec, LedgerBlockExecutor, LedgerStateCodec, LedgerStateNamespace,
    StateCommitmentPolicy, authenticated_state_root, state_root,
};
use network_config_auth_ed25519::Ed25519NetworkConfigVerifier;
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, SignedNetworkConfig,
    VerifiedNetworkConfig,
};
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, ManifestId, SyncCompletion,
    ValidatorSetHash,
};
use tail_state_store_lmdb::{
    CommitOutcome, LmdbStateStoreError, LmdbStoreOptions, LmdbTailStateStore,
};
use tail_sync_core::{
    FinalizedTailBlock, PreparedTailTransition, TailSyncSession, TailTransitionExecutor,
    VerifiedTailBlock,
};
use transaction_auth_ed25519::Ed25519Verifier;

const NETWORK: NetworkId = NetworkId::new([61; 32]);
const TOKEN: AssetId = AssetId::new([62; 32]);
const REGISTRY: AccountId = AccountId::new([63; 32]);
const ISSUER: AccountId = AccountId::new([64; 32]);
const BACKING: AccountId = AccountId::new([65; 32]);
const FREEZER: AccountId = AccountId::new([66; 32]);
const TREASURY: AccountId = AccountId::new([67; 32]);
const RECIPIENT: AccountId = AccountId::new([68; 32]);
const PROOF: &[u8] = b"finalized";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Finality;

impl FinalityProofVerifier for Finality {
    fn verify(&self, _network: NetworkId, _checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        proof == PROOF
    }
}

struct FixedTransition(PreparedTailTransition);

impl TailTransitionExecutor for FixedTransition {
    fn prepare_transition(
        &self,
        _previous: FinalizedCheckpoint,
        _previous_state: &[u8],
        _payload: &[u8],
    ) -> Option<PreparedTailTransition> {
        Some(self.0.clone())
    }
}

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ledger-state-lmdb-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn signer() -> SigningKey {
    SigningKey::from_bytes(&[69; 32])
}

fn verified_config(
    checkpoint: FinalizedCheckpoint,
    policy: StateCommitmentPolicy,
) -> VerifiedNetworkConfig {
    let key = SigningKey::from_bytes(&[75; 32]);
    let public_key = ConfigurationPublicKey::new(key.verifying_key().to_bytes());
    let config = NetworkConfig::new(NETWORK, checkpoint, 1, public_key, policy).unwrap();
    SignedNetworkConfig::new(
        config,
        ConfigurationSignature::new(key.sign(&config.signing_message()).to_bytes()),
    )
    .verify(NETWORK, public_key, &Ed25519NetworkConfigVerifier)
    .unwrap()
}

fn initial_ledger(sender: AccountId) -> Ledger {
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
    ledger.mint(ISSUER, TOKEN, sender, 10_000).unwrap();
    ledger
}

fn signed_transfer(
    key: &SigningKey,
    nonce: u64,
    idempotency_marker: u8,
    amount: u128,
) -> SignedOperation {
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new([idempotency_marker; 32]),
        asset: TOKEN,
        from: sender,
        to: RECIPIENT,
        amount,
        fee: 5,
        nonce,
        valid_until_height: 10_000,
    });
    let message = operation
        .authorization_message(AuthorizationRole::Sender)
        .unwrap();
    SignedOperation {
        operation,
        sender_authorization: Authorization {
            signer: sender,
            signature: SignatureBytes::new(key.sign(&message).to_bytes()),
        },
        fee_payer_authorization: None,
    }
}

fn verified_payment(
    executor: &LedgerBlockExecutor<Ed25519Verifier>,
    key: &SigningKey,
    previous: FinalizedCheckpoint,
    previous_state: &[u8],
    nonce: u64,
    marker: u8,
    amount: u128,
) -> VerifiedTailBlock {
    let payload = LedgerBlockCodec::encode(&[signed_transfer(key, nonce, marker, amount)]).unwrap();
    verified_payload(executor, previous, previous_state, payload)
}

fn verified_payload(
    executor: &LedgerBlockExecutor<Ed25519Verifier>,
    previous: FinalizedCheckpoint,
    previous_state: &[u8],
    payload: Vec<u8>,
) -> VerifiedTailBlock {
    let prepared = executor
        .execute(previous, previous_state, &payload)
        .unwrap();
    let block = FinalizedTailBlock {
        network: NETWORK,
        parent_hash: previous.block_hash,
        checkpoint: FinalizedCheckpoint {
            height: previous.height + 1,
            block_hash: prepared.transition.commitment.block_hash,
            state_root: prepared.transition.commitment.state_root,
            validator_set_hash: previous.validator_set_hash,
        },
        payload,
        finality_proof: PROOF.to_vec(),
    };
    TailSyncSession::new(NETWORK, previous)
        .verify_next(block, previous_state, &Finality, executor)
        .unwrap()
}

fn assert_balances(
    snapshot: ledger_core::LedgerSnapshot,
    sender: AccountId,
    sender_balance: u128,
    recipient_balance: u128,
    treasury_balance: u128,
    nonce: u64,
) {
    let ledger = Ledger::from_snapshot(NETWORK, snapshot).unwrap();
    assert_eq!(ledger.balance(TOKEN, sender), sender_balance);
    assert_eq!(ledger.balance(TOKEN, RECIPIENT), recipient_balance);
    assert_eq!(ledger.balance(TOKEN, TREASURY), treasury_balance);
    assert_eq!(ledger.nonce(sender), nonce);
}

fn assert_persisted_proofs(store: &LmdbTailStateStore, sender: AccountId) {
    let sender_key = [TOKEN.as_bytes().as_slice(), sender.as_bytes().as_slice()].concat();
    let base = store
        .prove_ledger_state(100, LedgerStateNamespace::Balance, &sender_key)
        .unwrap();
    let latest = store
        .prove_ledger_state(102, LedgerStateNamespace::Balance, &sender_key)
        .unwrap();
    assert_eq!(
        base.proof.value(),
        Some(10_000_u128.to_be_bytes().as_slice())
    );
    assert_eq!(
        latest.proof.value(),
        Some(7_740_u128.to_be_bytes().as_slice())
    );
    assert!(base.verifies());
    assert!(latest.verifies());

    let missing_key = [TOKEN.as_bytes().as_slice(), RECIPIENT.as_bytes().as_slice()].concat();
    let missing = store
        .prove_ledger_state(100, LedgerStateNamespace::Balance, &missing_key)
        .unwrap();
    assert_eq!(missing.proof.value(), None);
    assert!(missing.verifies());
    assert!(matches!(
        store.prove_ledger_state(99, LedgerStateNamespace::Balance, &sender_key),
        Err(tail_state_store_lmdb::LmdbStateStoreError::CursorMismatch)
    ));
}

fn initialize_and_commit_activation_block(
    path: &PathBuf,
    config: VerifiedNetworkConfig,
    base: FinalizedCheckpoint,
    initial_snapshot: &ledger_core::LedgerSnapshot,
    verified: &VerifiedTailBlock,
    sender: AccountId,
) {
    let store = LmdbTailStateStore::open_ledger(path, LmdbStoreOptions::default(), config).unwrap();
    store
        .initialize_ledger_from_snapshot(
            SyncCompletion {
                manifest: ManifestId::new([73; 32]),
                checkpoint: base,
            },
            initial_snapshot,
        )
        .unwrap();
    assert_eq!(
        store.current_authenticated_state().unwrap(),
        tail_state_store_lmdb::StoredAuthenticatedState {
            base_height: 100,
            latest_height: 100,
            tree_version: 0,
            root: authenticated_state_root(initial_snapshot).unwrap(),
        }
    );
    assert_eq!(
        store.commit_verified_ledger(verified).unwrap(),
        CommitOutcome::Committed
    );
    assert_eq!(
        store.commit_verified_ledger(verified).unwrap(),
        CommitOutcome::ExistingSame
    );
    let current = store.current_ledger().unwrap();
    assert_eq!(current.checkpoint, verified.checkpoint());
    assert_eq!(
        current.checkpoint.state_root,
        authenticated_state_root(&current.snapshot).unwrap()
    );
    assert_eq!(
        store.current_authenticated_state().unwrap(),
        tail_state_store_lmdb::StoredAuthenticatedState {
            base_height: 100,
            latest_height: 101,
            tree_version: 1,
            root: authenticated_state_root(&current.snapshot).unwrap(),
        }
    );
    assert_balances(current.snapshot, sender, 8_745, 1_250, 5, 1);
    assert_eq!(store.current().unwrap().state, verified.state());
    assert_eq!(store.finalized_payload(101).unwrap(), verified.payload());
}

#[test]
fn normalized_ledger_block_commit_is_atomic_restart_safe_and_idempotent() {
    let path = directory("restart");
    let key = signer();
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let initial_snapshot = initial_ledger(sender).snapshot();
    let initial_state = LedgerStateCodec::encode(&initial_snapshot).unwrap();
    let base = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([71; 32]),
        state_root: state_root(&initial_state),
        validator_set_hash: ValidatorSetHash::new([72; 32]),
    };
    let policy = StateCommitmentPolicy::authenticated_state_v1_from(101);
    let config = verified_config(base, policy);
    let executor =
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).with_state_commitment_policy(policy);
    let verified = verified_payment(&executor, &key, base, &initial_state, 0, 70, 1_250);
    initialize_and_commit_activation_block(
        &path,
        config,
        base,
        &initial_snapshot,
        &verified,
        sender,
    );
    assert_eq!(
        LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap_err(),
        LmdbStateStoreError::VerifiedNetworkConfigRequired
    );
    assert_eq!(
        LmdbTailStateStore::open_ledger(
            &path,
            LmdbStoreOptions::default(),
            verified_config(
                base,
                StateCommitmentPolicy::authenticated_state_v1_from(102),
            ),
        )
        .unwrap_err(),
        LmdbStateStoreError::NetworkConfigMismatch
    );
    let reopened =
        LmdbTailStateStore::open_ledger(&path, LmdbStoreOptions::default(), config).unwrap();
    assert_eq!(reopened.finalized_payload(101).unwrap(), verified.payload());
    let recovered_base = reopened.recovery_base_ledger().unwrap();
    assert_eq!(recovered_base.checkpoint, base);
    assert_eq!(recovered_base.snapshot, initial_snapshot);
    assert_eq!(reopened.current_ledger().unwrap().checkpoint.height, 101);
    assert_eq!(
        reopened
            .current_authenticated_state()
            .unwrap()
            .latest_height,
        101
    );
    assert_eq!(
        reopened.current_authenticated_state().unwrap().tree_version,
        1
    );
    let current = reopened.current().unwrap();
    let second_verified = verified_payment(
        &executor,
        &key,
        current.checkpoint,
        &current.state,
        1,
        74,
        1_000,
    );
    assert_eq!(
        reopened.commit_verified_ledger(&second_verified).unwrap(),
        CommitOutcome::Committed
    );
    let final_state = reopened.current_ledger().unwrap();
    assert_eq!(
        final_state.checkpoint.state_root,
        authenticated_state_root(&final_state.snapshot).unwrap()
    );
    assert_balances(final_state.snapshot, sender, 7_740, 2_250, 10, 2);
    assert_eq!(
        reopened.current_authenticated_state().unwrap().tree_version,
        2
    );
    assert_persisted_proofs(&reopened, sender);
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn unchanged_block_does_not_prune_the_recovery_base_state() {
    let path = directory("unchanged-base-state");
    let key = signer();
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let initial_snapshot = initial_ledger(sender).snapshot();
    let initial_state = LedgerStateCodec::encode(&initial_snapshot).unwrap();
    let policy = StateCommitmentPolicy::authenticated_state_v1_from(100);
    let base = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([80; 32]),
        state_root: policy.root_for_state(100, NETWORK, &initial_state).unwrap(),
        validator_set_hash: ValidatorSetHash::new([72; 32]),
    };
    let config = verified_config(base, policy);
    let executor =
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).with_state_commitment_policy(policy);
    let store =
        LmdbTailStateStore::open_ledger(&path, LmdbStoreOptions::default(), config).unwrap();
    store
        .initialize_ledger_from_snapshot(
            SyncCompletion {
                manifest: ManifestId::new([81; 32]),
                checkpoint: base,
            },
            &initial_snapshot,
        )
        .unwrap();

    let empty = verified_payload(
        &executor,
        base,
        &initial_state,
        LedgerBlockCodec::encode(&[]).unwrap(),
    );
    store.commit_verified_ledger(&empty).unwrap();
    let unchanged = store.current().unwrap();
    let payment = verified_payment(
        &executor,
        &key,
        unchanged.checkpoint,
        &unchanged.state,
        0,
        82,
        1_000,
    );
    store.commit_verified_ledger(&payment).unwrap();
    drop(store);

    let reopened =
        LmdbTailStateStore::open_ledger(&path, LmdbStoreOptions::default(), config).unwrap();
    assert_eq!(reopened.recovery_base_ledger().unwrap().checkpoint, base);
    assert_eq!(
        reopened.current_ledger().unwrap().checkpoint,
        payment.checkpoint()
    );
    assert_eq!(reopened.finalized_payload(101).unwrap(), empty.payload());
    assert_balances(
        reopened.current_ledger().unwrap().snapshot,
        sender,
        8_995,
        1_000,
        5,
        1,
    );
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn authenticated_root_mismatch_rolls_back_the_complete_lmdb_transaction() {
    let path = directory("wrong-authenticated-root");
    let key = signer();
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let initial_snapshot = initial_ledger(sender).snapshot();
    let initial_state = LedgerStateCodec::encode(&initial_snapshot).unwrap();
    let base = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([76; 32]),
        state_root: state_root(&initial_state),
        validator_set_hash: ValidatorSetHash::new([72; 32]),
    };
    let policy = StateCommitmentPolicy::authenticated_state_v1_from(101);
    let store = LmdbTailStateStore::open_ledger(
        &path,
        LmdbStoreOptions::default(),
        verified_config(base, policy),
    )
    .unwrap();
    store
        .initialize_ledger_from_snapshot(
            SyncCompletion {
                manifest: ManifestId::new([77; 32]),
                checkpoint: base,
            },
            &initial_snapshot,
        )
        .unwrap();
    let payload = LedgerBlockCodec::encode(&[signed_transfer(&key, 0, 78, 100)]).unwrap();
    let executed = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .with_state_commitment_policy(policy)
        .execute(base, &initial_state, &payload)
        .unwrap();
    let wrong_root = state_sync_core::StateRoot::new([79; 32]);
    let fixed = FixedTransition(PreparedTailTransition {
        commitment: tail_sync_core::TailCommitment {
            block_hash: executed.transition.commitment.block_hash,
            state_root: wrong_root,
        },
        state: executed.transition.state,
    });
    let block = FinalizedTailBlock {
        network: NETWORK,
        parent_hash: base.block_hash,
        checkpoint: FinalizedCheckpoint {
            height: 101,
            block_hash: fixed.0.commitment.block_hash,
            state_root: wrong_root,
            validator_set_hash: base.validator_set_hash,
        },
        payload,
        finality_proof: PROOF.to_vec(),
    };
    let verified = TailSyncSession::new(NETWORK, base)
        .verify_next(block, &initial_state, &Finality, &fixed)
        .unwrap();

    assert_eq!(
        store.commit_verified_ledger(&verified),
        Err(LmdbStateStoreError::InvalidLedgerState)
    );
    assert_eq!(store.current().unwrap().checkpoint, base);
    assert_eq!(store.current().unwrap().state, initial_state);
    assert_eq!(
        store.current_authenticated_state().unwrap().latest_height,
        base.height
    );

    drop(store);
    fs::remove_dir_all(path).unwrap();
}
