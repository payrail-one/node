use payment_idempotency_core::ReconciliationPageLimit;
use payment_reconciliation_work_core::LeaseDuration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pending_delay_ms: u64,
    error_base_delay_ms: u64,
    error_max_delay_ms: u64,
    alert_after_failures: u32,
}

impl RetryPolicy {
    #[must_use]
    pub const fn new(
        pending_delay_ms: u64,
        error_base_delay_ms: u64,
        error_max_delay_ms: u64,
        alert_after_failures: u32,
    ) -> Option<Self> {
        if pending_delay_ms == 0
            || error_base_delay_ms == 0
            || error_max_delay_ms < error_base_delay_ms
            || alert_after_failures == 0
        {
            None
        } else {
            Some(Self {
                pending_delay_ms,
                error_base_delay_ms,
                error_max_delay_ms,
                alert_after_failures,
            })
        }
    }

    #[must_use]
    pub const fn pending_delay_ms(self) -> u64 {
        self.pending_delay_ms
    }

    #[must_use]
    pub fn error_delay_ms(self, failure_number: u32) -> u64 {
        let shift = failure_number.saturating_sub(1).min(63);
        let factor = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
        self.error_base_delay_ms
            .saturating_mul(factor)
            .min(self.error_max_delay_ms)
    }

    #[must_use]
    pub const fn should_alert(self, consecutive_failures: u32) -> bool {
        consecutive_failures >= self.alert_after_failures
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationWorkerConfig {
    pub lease_duration: LeaseDuration,
    pub page_limit: ReconciliationPageLimit,
    pub retry_policy: RetryPolicy,
}
