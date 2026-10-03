use ledger_core::{LedgerError, NetworkId, SignatureVerifier, SignedOperation, VerifiedOperation};
use transaction_verification_core::VerificationError;
use transaction_verification_core::VerifiedOperationCache;

pub use transaction_verification_core::{
    DEFAULT_PARALLEL_SIGNATURE_THRESHOLD, MAX_SIGNATURE_WORKERS, SignatureVerificationPolicy,
};

use crate::RuntimeError;

pub(crate) fn verify_operations<V: SignatureVerifier + Sync>(
    network: NetworkId,
    verifier: &V,
    operations: Vec<SignedOperation>,
    policy: SignatureVerificationPolicy,
    cache: Option<&VerifiedOperationCache>,
) -> Result<Vec<Result<VerifiedOperation, LedgerError>>, RuntimeError> {
    transaction_verification_core::verify_operations_with_cache(
        network, verifier, operations, policy, cache,
    )
    .map_err(|error| match error {
        VerificationError::InvalidPolicy => RuntimeError::InvalidSignatureVerificationPolicy,
        VerificationError::WorkerFailed => RuntimeError::SignatureWorkerFailed,
    })
}
