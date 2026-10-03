use payment_idempotency_core::{PaymentRequestId, RequestDigest};

pub const MAX_LEASE_DURATION_MS: u64 = 3_600_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorkerId([u8; 32]);

impl WorkerId {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Option<Self> {
        if all_zero(&value) {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseDuration(u64);

impl LeaseDuration {
    #[must_use]
    pub const fn new(value_ms: u64) -> Option<Self> {
        if value_ms == 0 || value_ms > MAX_LEASE_DURATION_MS {
            None
        } else {
            Some(Self(value_ms))
        }
    }

    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LeaseToken(u64);

impl LeaseToken {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationLease {
    pub owner: WorkerId,
    pub token: LeaseToken,
    pub expires_at_ms: u64,
}

impl ReconciliationLease {
    #[must_use]
    pub const fn is_active_at(self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseAcquireOutcome {
    Acquired(ReconciliationLease),
    Busy { expires_at_ms: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledReconciliation {
    pub request_id: PaymentRequestId,
    pub request_digest: RequestDigest,
    pub consecutive_failures: u32,
    pub next_attempt_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeferReason {
    Pending,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationWorkError {
    StaleLease,
    InvalidSchedule,
    ScheduleChanged,
    TimeNotFuture,
    ArithmeticOverflow,
}

const fn all_zero(value: &[u8; 32]) -> bool {
    let mut index = 0;
    while index < value.len() {
        if value[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}
