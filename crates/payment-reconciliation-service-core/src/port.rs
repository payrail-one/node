use crate::ReconciliationTelemetryEvent;

pub trait ReconciliationClock {
    type Error;

    /// Returns Unix time in milliseconds. Implementations must not move
    /// backwards during one worker cycle.
    ///
    /// # Errors
    ///
    /// Returns the clock adapter error when time is unavailable.
    fn now_ms(&self) -> Result<u64, Self::Error>;
}

pub trait ReconciliationTelemetry {
    fn record(&self, event: ReconciliationTelemetryEvent);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopReconciliationTelemetry;

impl ReconciliationTelemetry for NoopReconciliationTelemetry {
    fn record(&self, _event: ReconciliationTelemetryEvent) {}
}
