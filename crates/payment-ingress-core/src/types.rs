use ledger_core::AccountId;
use payment_idempotency_core::TenantId;

use crate::RateLimitConfigError;

/// Opaque identity derived from authenticated API credentials.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ApiPrincipalId([u8; 32]);

impl ApiPrincipalId {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Authenticated and authorized scope supplied by a transport adapter.
///
/// The ingress service verifies that `tenant` and `account` match the canonical
/// payment request. Callers must not construct this value from request fields
/// without first applying their authentication and authorization policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentIngressIdentity {
    pub principal: ApiPrincipalId,
    pub tenant: TenantId,
    pub account: AccountId,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RateLimitScope {
    ApiPrincipal,
    Tenant,
    Account,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BucketPolicy {
    capacity: u32,
    refill_interval_ms: u64,
}

impl BucketPolicy {
    /// Creates a one-token-per-interval bucket policy.
    ///
    /// # Errors
    ///
    /// Returns an error when capacity or the refill interval is zero, or when
    /// the complete refill duration cannot be represented.
    pub fn new(capacity: u32, refill_interval_ms: u64) -> Result<Self, RateLimitConfigError> {
        if capacity == 0 {
            return Err(RateLimitConfigError::ZeroBucketCapacity);
        }
        if refill_interval_ms == 0 {
            return Err(RateLimitConfigError::ZeroRefillInterval);
        }
        if u64::from(capacity)
            .checked_mul(refill_interval_ms)
            .is_none()
        {
            return Err(RateLimitConfigError::RecoveryWindowOverflow);
        }
        Ok(Self {
            capacity,
            refill_interval_ms,
        })
    }

    #[must_use]
    pub const fn capacity(self) -> u32 {
        self.capacity
    }

    #[must_use]
    pub const fn refill_interval_ms(self) -> u64 {
        self.refill_interval_ms
    }

    pub(crate) fn full_recovery_ms(self) -> u64 {
        u64::from(self.capacity) * self.refill_interval_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrackingCapacities {
    pub principals: usize,
    pub tenants: usize,
    pub accounts: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentRateLimitConfig {
    principal: BucketPolicy,
    tenant: BucketPolicy,
    account: BucketPolicy,
    tracking: TrackingCapacities,
    idle_ttl_ms: u64,
}

impl PaymentRateLimitConfig {
    /// Builds a bounded three-scope rate-limit policy.
    ///
    /// The idle lifetime must be at least the longest complete bucket recovery
    /// window. Eviction can then never restore tokens earlier than ordinary
    /// refill would have restored them.
    ///
    /// # Errors
    ///
    /// Returns an error for zero tracking bounds, an unsafe idle lifetime or a
    /// lifetime that cannot be added to a monotonic timestamp.
    pub fn new(
        principal: BucketPolicy,
        tenant: BucketPolicy,
        account: BucketPolicy,
        tracking: TrackingCapacities,
        idle_ttl_ms: u64,
    ) -> Result<Self, RateLimitConfigError> {
        if tracking.principals == 0 {
            return Err(RateLimitConfigError::ZeroTrackingCapacity(
                RateLimitScope::ApiPrincipal,
            ));
        }
        if tracking.tenants == 0 {
            return Err(RateLimitConfigError::ZeroTrackingCapacity(
                RateLimitScope::Tenant,
            ));
        }
        if tracking.accounts == 0 {
            return Err(RateLimitConfigError::ZeroTrackingCapacity(
                RateLimitScope::Account,
            ));
        }
        let minimum = maximum(
            principal.full_recovery_ms(),
            maximum(tenant.full_recovery_ms(), account.full_recovery_ms()),
        );
        if idle_ttl_ms < minimum {
            return Err(RateLimitConfigError::IdleTtlTooShort {
                minimum_ms: minimum,
            });
        }
        Ok(Self {
            principal,
            tenant,
            account,
            tracking,
            idle_ttl_ms,
        })
    }

    #[must_use]
    pub const fn principal(self) -> BucketPolicy {
        self.principal
    }

    #[must_use]
    pub const fn tenant(self) -> BucketPolicy {
        self.tenant
    }

    #[must_use]
    pub const fn account(self) -> BucketPolicy {
        self.account
    }

    #[must_use]
    pub const fn tracking(self) -> TrackingCapacities {
        self.tracking
    }

    #[must_use]
    pub const fn idle_ttl_ms(self) -> u64 {
        self.idle_ttl_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitDecision {
    Admitted,
    Limited {
        scope: RateLimitScope,
        retry_at_ms: u64,
    },
}

const fn maximum(left: u64, right: u64) -> u64 {
    if left > right { left } else { right }
}
