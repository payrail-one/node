use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use ledger_core::{NetworkId, SignedOperation, VerifiedAuthorizationId, VerifiedOperation};

pub const DEFAULT_MAX_CACHED_AUTHORIZATIONS: usize = 16_384;
pub const DEFAULT_MAX_CACHED_AUTHORIZATION_BYTES: usize = 64 * 1024 * 1024;

pub type SharedVerifiedOperationCache = Arc<VerifiedOperationCache>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedOperationCacheError {
    InvalidLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInsertion {
    Inserted,
    AlreadyPresent,
    SkippedOversized,
    IdentityCollision,
    WrongNetwork,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedOperationCacheLimits {
    pub maximum_operations: usize,
    pub maximum_encoded_bytes: usize,
}

impl Default for VerifiedOperationCacheLimits {
    fn default() -> Self {
        Self {
            maximum_operations: DEFAULT_MAX_CACHED_AUTHORIZATIONS,
            maximum_encoded_bytes: DEFAULT_MAX_CACHED_AUTHORIZATION_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VerifiedOperationCacheSnapshot {
    pub operations: usize,
    pub encoded_bytes: usize,
}

#[derive(Debug)]
struct CacheEntry {
    verified: VerifiedOperation,
    encoded_bytes: usize,
}

#[derive(Debug, Default)]
struct CacheState {
    entries: BTreeMap<VerifiedAuthorizationId, CacheEntry>,
    admission_order: VecDeque<VerifiedAuthorizationId>,
    encoded_bytes: usize,
}

/// Bounded process-local cache of independently verified authorizations.
///
/// Values cannot be decoded from the network or disk because the cached type
/// can only be constructed by `ledger-core` verification. Lookup additionally
/// compares the complete signed operation after the hash match, making a hash
/// collision a cache miss rather than an authorization bypass.
#[derive(Debug)]
pub struct VerifiedOperationCache {
    network: NetworkId,
    limits: VerifiedOperationCacheLimits,
    state: Mutex<CacheState>,
}

impl VerifiedOperationCache {
    /// Creates an empty cache bound to one immutable network identity.
    ///
    /// # Errors
    ///
    /// Returns an error when either capacity is zero.
    pub fn new(
        network: NetworkId,
        limits: VerifiedOperationCacheLimits,
    ) -> Result<Self, VerifiedOperationCacheError> {
        if limits.maximum_operations == 0 || limits.maximum_encoded_bytes == 0 {
            return Err(VerifiedOperationCacheError::InvalidLimits);
        }
        Ok(Self {
            network,
            limits,
            state: Mutex::new(CacheState::default()),
        })
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Records a capability created by the local authoritative verifier.
    ///
    /// Cache failure never changes transaction validity. Callers can safely
    /// continue and perform ordinary verification on a later cache miss.
    pub fn record(&self, verified: VerifiedOperation) -> CacheInsertion {
        self.record_many(vec![verified])
            .into_iter()
            .next()
            .unwrap_or(CacheInsertion::Unavailable)
    }

    /// Records a verified batch while holding the bounded cache lock once.
    #[must_use]
    pub fn record_many(&self, verified: Vec<VerifiedOperation>) -> Vec<CacheInsertion> {
        let prepared = verified
            .into_iter()
            .map(|verified| {
                if verified.network() != self.network {
                    return Err(CacheInsertion::WrongNetwork);
                }
                let identity = verified
                    .signed()
                    .verified_authorization_identity()
                    .map_err(|_| CacheInsertion::SkippedOversized)?;
                if identity.encoded_bytes() > self.limits.maximum_encoded_bytes {
                    return Err(CacheInsertion::SkippedOversized);
                }
                Ok((verified, identity))
            })
            .collect::<Vec<_>>();
        let Ok(mut state) = self.state.lock() else {
            return prepared
                .into_iter()
                .map(|_| CacheInsertion::Unavailable)
                .collect();
        };
        prepared
            .into_iter()
            .map(|prepared| {
                let (verified, identity) = match prepared {
                    Ok(prepared) => prepared,
                    Err(outcome) => return outcome,
                };
                if let Some(existing) = state.entries.get(&identity.id()) {
                    return if existing.verified.signed() == verified.signed() {
                        CacheInsertion::AlreadyPresent
                    } else {
                        CacheInsertion::IdentityCollision
                    };
                }
                while state.entries.len() >= self.limits.maximum_operations
                    || state
                        .encoded_bytes
                        .checked_add(identity.encoded_bytes())
                        .is_none_or(|bytes| bytes > self.limits.maximum_encoded_bytes)
                {
                    let Some(oldest) = state.admission_order.pop_front() else {
                        return CacheInsertion::Unavailable;
                    };
                    if let Some(removed) = state.entries.remove(&oldest) {
                        state.encoded_bytes =
                            state.encoded_bytes.saturating_sub(removed.encoded_bytes);
                    }
                }
                state.encoded_bytes = state.encoded_bytes.saturating_add(identity.encoded_bytes());
                state.admission_order.push_back(identity.id());
                state.entries.insert(
                    identity.id(),
                    CacheEntry {
                        verified,
                        encoded_bytes: identity.encoded_bytes(),
                    },
                );
                CacheInsertion::Inserted
            })
            .collect()
    }

    /// Returns capabilities only for byte-for-byte identical signed operations.
    #[must_use]
    pub fn lookup_many(&self, signed: &[SignedOperation]) -> Vec<Option<VerifiedOperation>> {
        let identities = signed
            .iter()
            .map(|operation| {
                (operation.operation.network() == self.network)
                    .then(|| operation.verified_authorization_id().ok())
                    .flatten()
            })
            .collect::<Vec<_>>();
        let Ok(state) = self.state.lock() else {
            return signed.iter().map(|_| None).collect();
        };
        signed
            .iter()
            .zip(identities)
            .map(|(signed, identity)| {
                state.entries.get(&identity?).and_then(|entry| {
                    (entry.verified.signed() == signed).then(|| entry.verified.clone())
                })
            })
            .collect()
    }

    /// Returns a capability only for the byte-for-byte same signed operation.
    #[must_use]
    pub fn lookup(&self, signed: &SignedOperation) -> Option<VerifiedOperation> {
        self.lookup_many(std::slice::from_ref(signed))
            .into_iter()
            .next()
            .flatten()
    }

    #[must_use]
    pub fn snapshot(&self) -> VerifiedOperationCacheSnapshot {
        self.state.lock().map_or_else(
            |_| VerifiedOperationCacheSnapshot::default(),
            |state| VerifiedOperationCacheSnapshot {
                operations: state.entries.len(),
                encoded_bytes: state.encoded_bytes,
            },
        )
    }
}
