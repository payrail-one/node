use payment_gateway_core::PaymentRequestError;

use crate::RateLimitScope;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitConfigError {
    ZeroBucketCapacity,
    ZeroRefillInterval,
    RecoveryWindowOverflow,
    ZeroTrackingCapacity(RateLimitScope),
    IdleTtlTooShort { minimum_ms: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitError {
    ClockRegression { previous_ms: u64, now_ms: u64 },
    TimeOverflow,
    TrackingCapacityReached(RateLimitScope),
    AccountingInconsistent(RateLimitScope),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentIngressError<JournalError, LimiterError, GatewayError> {
    InvalidRequest(PaymentRequestError),
    WrongNetwork,
    IdentityMismatch,
    RequestConflict,
    Journal(JournalError),
    Limiter(LimiterError),
    RateLimited {
        scope: RateLimitScope,
        retry_at_ms: u64,
    },
    Gateway(GatewayError),
}

pub type PaymentIngressResult<Value, JournalError, LimiterError, GatewayError> =
    Result<Value, PaymentIngressError<JournalError, LimiterError, GatewayError>>;
