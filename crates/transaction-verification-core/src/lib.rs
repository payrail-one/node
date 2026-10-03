#![forbid(unsafe_code)]

mod cache;

use std::thread;

use ledger_core::{
    LedgerError, NetworkId, SignatureVerifier, SignedOperation, VerifiedOperation,
    verify_operation_batch,
};

pub use cache::{
    CacheInsertion, DEFAULT_MAX_CACHED_AUTHORIZATION_BYTES, DEFAULT_MAX_CACHED_AUTHORIZATIONS,
    SharedVerifiedOperationCache, VerifiedOperationCache, VerifiedOperationCacheError,
    VerifiedOperationCacheLimits, VerifiedOperationCacheSnapshot,
};

pub const MAX_SIGNATURE_WORKERS: usize = 64;
pub const DEFAULT_PARALLEL_SIGNATURE_THRESHOLD: usize = 64;
const RECOMMENDED_MAX_SIGNATURE_WORKERS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationError {
    InvalidPolicy,
    WorkerFailed,
}

/// Bounded local policy for independent transaction authorization checks.
///
/// It affects execution strategy only. Consensus-visible operation order,
/// receipts and state commitments do not depend on the selected worker count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignatureVerificationPolicy {
    max_workers: usize,
    parallel_threshold: usize,
}

impl SignatureVerificationPolicy {
    /// Creates an explicitly bounded verification policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero workers, excessive workers or a threshold
    /// smaller than two operations.
    pub const fn new(
        max_workers: usize,
        parallel_threshold: usize,
    ) -> Result<Self, VerificationError> {
        if max_workers == 0 || max_workers > MAX_SIGNATURE_WORKERS || parallel_threshold < 2 {
            return Err(VerificationError::InvalidPolicy);
        }
        Ok(Self {
            max_workers,
            parallel_threshold,
        })
    }

    #[must_use]
    pub const fn sequential() -> Self {
        Self {
            max_workers: 1,
            parallel_threshold: usize::MAX,
        }
    }

    #[must_use]
    pub fn recommended() -> Self {
        let available = thread::available_parallelism().map_or(1, usize::from);
        Self {
            max_workers: available.min(RECOMMENDED_MAX_SIGNATURE_WORKERS),
            parallel_threshold: DEFAULT_PARALLEL_SIGNATURE_THRESHOLD,
        }
    }

    #[must_use]
    pub const fn max_workers(self) -> usize {
        self.max_workers
    }

    #[must_use]
    pub const fn parallel_threshold(self) -> usize {
        self.parallel_threshold
    }
}

impl Default for SignatureVerificationPolicy {
    fn default() -> Self {
        Self::recommended()
    }
}

/// Verifies an ordered bounded collection without changing its result order.
///
/// Every worker is joined, including after another worker fails. A panic in a
/// cryptographic adapter therefore becomes a closed batch failure instead of
/// unwinding through the caller or returning a partial result.
///
/// # Errors
///
/// Returns `WorkerFailed` if any verification worker panics.
pub fn verify_operations<V: SignatureVerifier + Sync>(
    network: NetworkId,
    verifier: &V,
    operations: Vec<SignedOperation>,
    policy: SignatureVerificationPolicy,
) -> Result<Vec<Result<VerifiedOperation, LedgerError>>, VerificationError> {
    let worker_count = operation_worker_count(operations.len(), policy);
    if worker_count <= 1 {
        return Ok(verify_operation_batch(network, verifier, operations));
    }

    let chunk_size = operations.len().div_ceil(worker_count);
    let chunks = into_chunks(operations, chunk_size);
    thread::scope(|scope| {
        let handles = chunks
            .into_iter()
            .map(|chunk| scope.spawn(move || verify_operation_batch(network, verifier, chunk)))
            .collect::<Vec<_>>();
        let mut checked_operations = Vec::new();
        let mut worker_failed = false;
        for handle in handles {
            match handle.join() {
                Ok(mut chunk) => checked_operations.append(&mut chunk),
                Err(_) => worker_failed = true,
            }
        }
        if worker_failed {
            Err(VerificationError::WorkerFailed)
        } else {
            Ok(checked_operations)
        }
    })
}

/// Reuses only locally created authorization capabilities and verifies every
/// cache miss through the ordinary bounded worker path.
///
/// Monetary, nonce, expiry and state-dependent rules are intentionally outside
/// this function and must still be applied by the ordered ledger executor.
///
/// # Errors
///
/// Returns `WorkerFailed` if any verification worker for a cache miss panics.
pub fn verify_operations_with_cache<V: SignatureVerifier + Sync>(
    network: NetworkId,
    verifier: &V,
    operations: Vec<SignedOperation>,
    policy: SignatureVerificationPolicy,
    cache: Option<&VerifiedOperationCache>,
) -> Result<Vec<Result<VerifiedOperation, LedgerError>>, VerificationError> {
    let Some(cache) = cache.filter(|cache| cache.network() == network) else {
        return verify_operations(network, verifier, operations, policy);
    };
    let mut results = (0..operations.len()).map(|_| None).collect::<Vec<_>>();
    let mut miss_indexes = Vec::new();
    let mut misses = Vec::new();
    let cached = cache.lookup_many(&operations);
    for (index, (operation, verified)) in operations.into_iter().zip(cached).enumerate() {
        if let Some(verified) = verified {
            results[index] = Some(Ok(verified));
        } else {
            miss_indexes.push(index);
            misses.push(operation);
        }
    }
    let verified_misses = verify_operations(network, verifier, misses, policy)?;
    let cacheable = verified_misses
        .iter()
        .filter_map(|result| result.as_ref().ok().cloned())
        .collect();
    let _ = cache.record_many(cacheable);
    for (index, result) in miss_indexes.into_iter().zip(verified_misses) {
        results[index] = Some(result);
    }
    results
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(VerificationError::WorkerFailed)
}

fn operation_worker_count(operation_count: usize, policy: SignatureVerificationPolicy) -> usize {
    if operation_count < policy.parallel_threshold {
        return 1;
    }
    policy.max_workers.min(operation_count)
}

fn into_chunks(operations: Vec<SignedOperation>, chunk_size: usize) -> Vec<Vec<SignedOperation>> {
    let mut iterator = operations.into_iter();
    let mut chunks = Vec::new();
    loop {
        let chunk = iterator.by_ref().take(chunk_size).collect::<Vec<_>>();
        if chunk.is_empty() {
            return chunks;
        }
        chunks.push(chunk);
    }
}
