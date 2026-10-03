use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::ThreadId,
};

use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, LedgerError, NetworkId,
    SignatureBytes, SignatureVerifier, SignedOperation, Transfer,
};
use ledger_runtime_core::{
    LedgerBlockCodec, LedgerBlockExecutor, LedgerStateCodec, RuntimeError,
    SignatureVerificationPolicy, state_root,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, ValidatorSetHash};
use transaction_auth_ed25519::Ed25519Verifier;
use transaction_verification_core::{VerifiedOperationCache, VerifiedOperationCacheLimits};

const NETWORK: NetworkId = NetworkId::new([131; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([132; 32]);
const ASSET: AssetId = AssetId::new([133; 32]);
const REGISTRY: AccountId = AccountId::new([134; 32]);
const ISSUER: AccountId = AccountId::new([135; 32]);
const BACKING: AccountId = AccountId::new([136; 32]);
const FREEZER: AccountId = AccountId::new([137; 32]);
const TREASURY: AccountId = AccountId::new([138; 32]);
const RECIPIENT: AccountId = AccountId::new([139; 32]);

#[derive(Clone)]
struct RecordingVerifier {
    threads: Arc<Mutex<HashSet<ThreadId>>>,
}

impl SignatureVerifier for RecordingVerifier {
    fn verify(&self, signer: AccountId, message: &[u8], signature: SignatureBytes) -> bool {
        let Ok(mut threads) = self.threads.lock() else {
            return false;
        };
        threads.insert(std::thread::current().id());
        drop(threads);
        Ed25519Verifier.verify(signer, message, signature)
    }
}

struct PanickingVerifier;

impl SignatureVerifier for PanickingVerifier {
    fn verify(&self, _signer: AccountId, _message: &[u8], _signature: SignatureBytes) -> bool {
        panic!("injected verifier panic")
    }
}

#[derive(Clone, Debug, Default)]
struct CountingEdVerifier(Arc<AtomicUsize>);

impl CountingEdVerifier {
    fn calls(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl SignatureVerifier for CountingEdVerifier {
    fn verify(&self, signer: AccountId, message: &[u8], signature: SignatureBytes) -> bool {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ed25519Verifier.verify(signer, message, signature)
    }
}

fn fixture() -> (SigningKey, Vec<u8>, FinalizedCheckpoint) {
    let key = SigningKey::from_bytes(&[140; 32]);
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: ASSET,
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
    ledger.mint(ISSUER, ASSET, sender, 10_000).unwrap();
    let state = LedgerStateCodec::encode(&ledger.snapshot()).unwrap();
    let checkpoint = FinalizedCheckpoint {
        height: 7,
        block_hash: BlockHash::new([141; 32]),
        state_root: state_root(&state),
        validator_set_hash: ValidatorSetHash::new([142; 32]),
    };
    (key, state, checkpoint)
}

fn signed_transfer(key: &SigningKey, nonce: u64, amount: u128) -> SignedOperation {
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let mut idempotency = [143; 32];
    idempotency[24..].copy_from_slice(&nonce.to_be_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new(idempotency),
        asset: ASSET,
        from: sender,
        to: RECIPIENT,
        amount,
        fee: 1,
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

#[test]
fn bounded_parallel_verification_is_byte_identical_to_sequential_execution() {
    let (key, state, checkpoint) = fixture();
    let operations = (0_u64..16)
        .map(|nonce| signed_transfer(&key, nonce, 1))
        .collect::<Vec<_>>();
    let payload = LedgerBlockCodec::encode(&operations).unwrap();
    let sequential = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .with_signature_policy(SignatureVerificationPolicy::sequential())
        .execute(checkpoint, &state, &payload)
        .unwrap();

    let threads = Arc::new(Mutex::new(HashSet::new()));
    let verifier = RecordingVerifier {
        threads: Arc::clone(&threads),
    };
    let parallel = LedgerBlockExecutor::new(NETWORK, verifier)
        .with_signature_policy(SignatureVerificationPolicy::new(4, 2).unwrap())
        .execute(checkpoint, &state, &payload)
        .unwrap();

    assert_eq!(parallel, sequential);
    assert!(threads.lock().unwrap().len() > 1);
}

#[test]
fn consensus_error_order_is_independent_of_parallel_preverification() {
    let (key, state, checkpoint) = fixture();
    let first_state_failure = signed_transfer(&key, 0, 50_000);
    let mut later_invalid_signature = signed_transfer(&key, 1, 1);
    later_invalid_signature.sender_authorization.signature = SignatureBytes::new([0; 64]);
    let payload =
        LedgerBlockCodec::encode(&[first_state_failure, later_invalid_signature]).unwrap();
    let policy = SignatureVerificationPolicy::new(2, 2).unwrap();

    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
            .with_signature_policy(policy)
            .execute(checkpoint, &state, &payload),
        Err(RuntimeError::ExecutionFailed {
            operation_index: 0,
            source: LedgerError::InsufficientBalance,
        })
    );
}

#[test]
fn worker_failure_and_wrong_network_fail_closed() {
    let (key, state, checkpoint) = fixture();
    let payload =
        LedgerBlockCodec::encode(&[signed_transfer(&key, 0, 1), signed_transfer(&key, 1, 1)])
            .unwrap();
    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, PanickingVerifier)
            .with_signature_policy(SignatureVerificationPolicy::new(2, 2).unwrap())
            .execute(checkpoint, &state, &payload),
        Err(RuntimeError::SignatureWorkerFailed)
    );

    let operation = signed_transfer(&key, 0, 1);
    assert_eq!(
        operation.verify_for_network(OTHER_NETWORK, &Ed25519Verifier),
        Err(LedgerError::WrongNetwork)
    );
}

#[test]
fn cached_authorization_skips_only_crypto_and_reapplies_nonce_rules() {
    let (key, state, checkpoint) = fixture();
    let signed = signed_transfer(&key, 0, 1);
    let payload = LedgerBlockCodec::encode(std::slice::from_ref(&signed)).unwrap();
    let verifier = CountingEdVerifier::default();
    let cache = Arc::new(
        VerifiedOperationCache::new(NETWORK, VerifiedOperationCacheLimits::default()).unwrap(),
    );
    let capability = signed.verify_for_network(NETWORK, &verifier).unwrap();
    let _ = cache.record(capability);
    assert_eq!(verifier.calls(), 1);

    let executor = LedgerBlockExecutor::new(NETWORK, verifier.clone())
        .with_authorization_cache(Arc::clone(&cache));
    let first = executor.execute(checkpoint, &state, &payload).unwrap();
    assert_eq!(verifier.calls(), 1);

    let next_checkpoint = FinalizedCheckpoint {
        height: checkpoint.height + 1,
        block_hash: first.transition.commitment.block_hash,
        state_root: first.transition.commitment.state_root,
        validator_set_hash: checkpoint.validator_set_hash,
    };
    assert_eq!(
        executor.execute(next_checkpoint, &first.transition.state, &payload),
        Err(RuntimeError::ExecutionFailed {
            operation_index: 0,
            source: LedgerError::NonceMismatch {
                expected: 1,
                actual: 0,
            },
        })
    );
    assert_eq!(verifier.calls(), 1);
}
