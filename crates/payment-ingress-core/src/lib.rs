#![forbid(unsafe_code)]

mod error;
mod limiter;
mod port;
mod service;
mod types;

pub use error::{PaymentIngressError, PaymentIngressResult, RateLimitConfigError, RateLimitError};
pub use limiter::BoundedPaymentRateLimiter;
pub use port::{PaymentIngressJournal, PaymentIngressRateLimiter, PaymentSubmissionGateway};
pub use service::PaymentIngressService;
pub use types::{
    ApiPrincipalId, BucketPolicy, PaymentIngressIdentity, PaymentRateLimitConfig,
    RateLimitDecision, RateLimitScope, TrackingCapacities,
};
