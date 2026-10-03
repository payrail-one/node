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
    LedgerBlockCodec, LedgerBlockExecutor, LedgerStateCodec, StateCommitmentPolicy, state_root,
};
use network_config_auth_ed25519::Ed25519NetworkConfigVerifier;
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, SignedNetworkConfig,
    VerifiedNetworkConfig,
};
use receipt_index_lmdb::{LmdbReceiptIndex, ReceiptIndexStoreOptions};
use receipt_rebuild_core::rebuild_receipt_index;
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, ManifestId, SyncCompletion,
    ValidatorSetHash,
};
use tail_state_store_lmdb::{LmdbStoreOptions, LmdbTailStateStore};
use tail_sync_core::{FinalizedTailBlock, TailSyncSession, VerifiedTailBlock};
use transaction_auth_ed25519::Ed25519Verifier;

const NETWORK: NetworkId = NetworkId::new([31; 32]);
const TOKEN: AssetId = AssetId::new([32; 32]);
const REGISTRY: AccountId = AccountId::new([33; 32]);
const ISSUER: AccountId = AccountId::new([34; 32]);
const BACKING: AccountId = AccountId::new([35; 32]);
const FREEZER: AccountId = AccountId::new([36; 32]);
const TREASURY: AccountId = AccountId::new([37; 32]);
const RECIPIENT: AccountId = AccountId::new([38; 32]);
const PROOF: &[u8] = b"receipt-rebuild-finality";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Finality;

impl FinalityProofVerifier for Finality {
    fn verify(&self, network: NetworkId, _checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        network == NETWORK && proof == PROOF
    }
}

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "receipt-rebuild-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn verified_config(checkpoint: FinalizedCheckpoint) -> VerifiedNetworkConfig {
    let key = SigningKey::from_bytes(&[39; 32]);
    let public_key = ConfigurationPublicKey::new(key.verifying_key().to_bytes());
    let config = NetworkConfig::new(
        NETWORK,
        checkpoint,
        1,
        public_key,
        StateCommitmentPolicy::canonical_state_v1(),
    )
    .unwrap();
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

fn signed_transfer(key: &SigningKey, nonce: u64, marker: u8) -> SignedOperation {
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new([marker; 32]),
        asset: TOKEN,
        from: sender,
        to: RECIPIENT,
        amount: 100,
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
) -> VerifiedTailBlock {
    let payload = LedgerBlockCodec::encode(&[signed_transfer(key, nonce, marker)]).unwrap();
    let executed = executor
        .execute(previous, previous_state, &payload)
        .unwrap();
    let block = FinalizedTailBlock {
        network: NETWORK,
        parent_hash: previous.block_hash,
        checkpoint: FinalizedCheckpoint {
            height: previous.height + 1,
            block_hash: executed.transition.commitment.block_hash,
            state_root: executed.transition.commitment.state_root,
            validator_set_hash: previous.validator_set_hash,
        },
        payload,
        finality_proof: PROOF.to_vec(),
    };
    TailSyncSession::new(NETWORK, previous)
        .verify_next(block, previous_state, &Finality, executor)
        .unwrap()
}

fn populated_source(
    path: &PathBuf,
) -> (
    LmdbTailStateStore,
    LedgerBlockExecutor<Ed25519Verifier>,
    FinalizedCheckpoint,
) {
    let key = SigningKey::from_bytes(&[40; 32]);
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let snapshot = initial_ledger(sender).snapshot();
    let state = LedgerStateCodec::encode(&snapshot).unwrap();
    let base = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([41; 32]),
        state_root: state_root(&state),
        validator_set_hash: ValidatorSetHash::new([42; 32]),
    };
    let store =
        LmdbTailStateStore::open_ledger(path, LmdbStoreOptions::default(), verified_config(base))
            .unwrap();
    store
        .initialize_ledger_from_snapshot(
            SyncCompletion {
                manifest: ManifestId::new([43; 32]),
                checkpoint: base,
            },
            &snapshot,
        )
        .unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);
    let first = verified_payment(&executor, &key, base, &state, 0, 44);
    store.commit_verified_ledger(&first).unwrap();
    let second = verified_payment(&executor, &key, first.checkpoint(), first.state(), 1, 45);
    store.commit_verified_ledger(&second).unwrap();
    (store, executor, second.checkpoint())
}

#[test]
fn real_signed_finalized_history_rebuilds_and_resumes_idempotently() {
    let source_path = directory("source");
    let index_path = directory("index");
    let (source, executor, final_checkpoint) = populated_source(&source_path);
    let index =
        LmdbReceiptIndex::open(&index_path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();

    let first = rebuild_receipt_index(&source, &index, &executor).unwrap();
    assert_eq!(first.base_height, 100);
    assert_eq!(first.final_height, 102);
    assert_eq!(first.replayed_blocks, 2);
    assert_eq!(first.verified_existing_blocks, 0);
    assert_eq!(first.newly_indexed_blocks, 2);
    assert_eq!(first.newly_indexed_receipts, 2);
    assert_eq!(first.final_operation_index, 2);
    assert_eq!(index.cursor().unwrap().latest(), final_checkpoint);
    assert_eq!(
        index.by_operation_index(0).unwrap().unwrap().receipt.nonce,
        0
    );
    assert_eq!(
        index.by_operation_index(1).unwrap().unwrap().receipt.nonce,
        1
    );

    drop(index);
    let reopened =
        LmdbReceiptIndex::open(&index_path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
    let resumed = rebuild_receipt_index(&source, &reopened, &executor).unwrap();
    assert_eq!(resumed.replayed_blocks, 2);
    assert_eq!(resumed.verified_existing_blocks, 2);
    assert_eq!(resumed.newly_indexed_blocks, 0);
    assert_eq!(resumed.newly_indexed_receipts, 0);
    assert_eq!(resumed.final_operation_index, 2);

    drop(reopened);
    drop(source);
    fs::remove_dir_all(source_path).unwrap();
    fs::remove_dir_all(index_path).unwrap();
}
