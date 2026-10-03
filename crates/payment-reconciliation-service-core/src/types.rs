#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReconciliationCycleReport {
    pub scanned: u32,
    pub finalized: u32,
    pub pending: u32,
    pub failed: u32,
    pub alerts: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationCycleOutcome {
    LeaseBusy { expires_at_ms: u64 },
    Processed(ReconciliationCycleReport),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationTelemetryEvent {
    LeaseBusy,
    RetryThresholdReached { consecutive_failures: u32 },
    CycleCompleted(ReconciliationCycleReport),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReconciliationWorkerError<WorkError, ClockError> {
    Work(WorkError),
    Clock(ClockError),
    WrongNetwork,
    ClockRegression,
    ArithmeticOverflow,
    CounterOverflow,
}
