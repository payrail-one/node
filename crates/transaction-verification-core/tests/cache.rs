use std::sync::atomic::{AtomicUsize, Ordering};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, LedgerError, NetworkId,
    SignatureBytes, SignatureVerifier, SignedOperation, Transfer,
};
use transaction_verification_core::{
    CacheInsertion, SignatureVerificationPolicy, VerifiedOperationCache,
    VerifiedOperationCacheError, VerifiedOperationCacheLimits, verify_operations_with_cache,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([2; 32]);
const SENDER: AccountId = AccountId::new([3; 32]);
const RECIPIENT: AccountId = AccountId::new([4; 32]);
const ASSET: AssetId = AssetId::new([5; 32]);
const VALID_SIGNATURE: SignatureBytes = SignatureBytes::new([6; 64]);

#[derive(Debug, Default)]
struct CountingVerifier(AtomicUsize);

impl CountingVerifier {
    fn calls(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl SignatureVerifier for CountingVerifier {
    fn verify(&self, signer: AccountId, _message: &[u8], signature: SignatureBytes) -> bool {
        self.0.fetch_add(1, Ordering::Relaxed);
        signer == SENDER && signature == VALID_SIGNATURE
    }
}

fn signed(network: NetworkId, nonce: u64, signature: SignatureBytes) -> SignedOperation {
    SignedOperation {
        operation: AuthorizedOperation::Transfer(Transfer {
            network,
            idempotency_key: IdempotencyKey::new([u8::try_from(nonce).unwrap_or(u8::MAX); 32]),
            asset: ASSET,
            from: SENDER,
            to: RECIPIENT,
            amount: 1,
            fee: 0,
            nonce,
            valid_until_height: 100,
        }),
        sender_authorization: Authorization {
            signer: SENDER,
            signature,
        },
        fee_payer_authorization: None,
    }
}

fn cache_with(limits: VerifiedOperationCacheLimits) -> VerifiedOperationCache {
    VerifiedOperationCache::new(NETWORK, limits).unwrap()
}

#[test]
fn exact_capability_avoids_repeat_crypto_but_changed_signature_is_a_miss() {
    let verifier = CountingVerifier::default();
    let cache = cache_with(VerifiedOperationCacheLimits::default());
    let valid = signed(NETWORK, 0, VALID_SIGNATURE);
    let first = verify_operations_with_cache(
        NETWORK,
        &verifier,
        vec![valid.clone()],
        SignatureVerificationPolicy::sequential(),
        Some(&cache),
    )
    .unwrap();
    assert!(first[0].is_ok());
    assert_eq!(verifier.calls(), 1);

    let second = verify_operations_with_cache(
        NETWORK,
        &verifier,
        vec![valid.clone()],
        SignatureVerificationPolicy::sequential(),
        Some(&cache),
    )
    .unwrap();
    assert!(second[0].is_ok());
    assert_eq!(verifier.calls(), 1);

    let forged = signed(NETWORK, 0, SignatureBytes::new([7; 64]));
    assert_eq!(
        valid.operation.operation_id().unwrap(),
        forged.operation.operation_id().unwrap()
    );
    assert_ne!(
        valid.verified_authorization_id().unwrap(),
        forged.verified_authorization_id().unwrap()
    );
    let rejected = verify_operations_with_cache(
        NETWORK,
        &verifier,
        vec![forged],
        SignatureVerificationPolicy::sequential(),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(rejected[0], Err(LedgerError::InvalidSignature));
    // A failed batch equation is deliberately followed by individual
    // verification to preserve deterministic error positions.
    assert_eq!(verifier.calls(), 3);
}

#[test]
fn cache_is_network_bound_and_never_converts_wrong_network_to_a_hit() {
    let verifier = CountingVerifier::default();
    let cache = cache_with(VerifiedOperationCacheLimits::default());
    let valid = signed(NETWORK, 0, VALID_SIGNATURE)
        .verify_for_network(NETWORK, &verifier)
        .unwrap();
    assert_eq!(cache.record(valid), CacheInsertion::Inserted);
    let foreign = signed(OTHER_NETWORK, 0, VALID_SIGNATURE);
    assert_eq!(cache.lookup(&foreign), None);
    let result = verify_operations_with_cache(
        NETWORK,
        &verifier,
        vec![foreign],
        SignatureVerificationPolicy::sequential(),
        Some(&cache),
    )
    .unwrap();
    assert_eq!(result[0], Err(LedgerError::WrongNetwork));
}

#[test]
fn entry_and_byte_limits_evict_or_skip_without_affecting_validity() {
    let verifier = CountingVerifier::default();
    let cache = cache_with(VerifiedOperationCacheLimits {
        maximum_operations: 1,
        maximum_encoded_bytes: 1_024,
    });
    let first_signed = signed(NETWORK, 0, VALID_SIGNATURE);
    let second_signed = signed(NETWORK, 1, VALID_SIGNATURE);
    let first = first_signed
        .clone()
        .verify_for_network(NETWORK, &verifier)
        .unwrap();
    let second = second_signed
        .clone()
        .verify_for_network(NETWORK, &verifier)
        .unwrap();
    assert_eq!(cache.record(first), CacheInsertion::Inserted);
    assert_eq!(cache.record(second), CacheInsertion::Inserted);
    assert_eq!(cache.lookup(&first_signed), None);
    assert!(cache.lookup(&second_signed).is_some());
    assert_eq!(cache.snapshot().operations, 1);

    let tiny = cache_with(VerifiedOperationCacheLimits {
        maximum_operations: 1,
        maximum_encoded_bytes: 1,
    });
    let capability = first_signed.verify_for_network(NETWORK, &verifier).unwrap();
    assert_eq!(tiny.record(capability), CacheInsertion::SkippedOversized);
    assert_eq!(tiny.snapshot().operations, 0);
}

#[test]
fn zero_cache_limits_are_rejected() {
    assert_eq!(
        VerifiedOperationCache::new(
            NETWORK,
            VerifiedOperationCacheLimits {
                maximum_operations: 0,
                maximum_encoded_bytes: 1,
            }
        )
        .unwrap_err(),
        VerifiedOperationCacheError::InvalidLimits
    );
}
