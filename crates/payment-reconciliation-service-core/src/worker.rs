use payment_idempotency_core::PaymentIdempotencyStore;
use payment_reconciliation_core::{PaymentFinalityReconciler, PaymentReconciliationOutcome};
use payment_reconciliation_work_core::{
    DeferReason, LeaseAcquireOutcome, ReconciliationLease, ReconciliationWorkStore,
    ScheduledReconciliation, WorkerId,
};
use receipt_index_core::ReceiptIndexStore;

use crate::{
    ReconciliationClock, ReconciliationCycleOutcome, ReconciliationCycleReport,
    ReconciliationTelemetry, ReconciliationTelemetryEvent, ReconciliationWorkerConfig,
    ReconciliationWorkerError,
};

#[derive(Clone, Copy, Debug)]
pub struct ReconciliationWorker<'a, Payments, Receipts, Work, Clock, Telemetry> {
    payments: &'a Payments,
    receipts: &'a Receipts,
    work: &'a Work,
    clock: &'a Clock,
    telemetry: &'a Telemetry,
    config: ReconciliationWorkerConfig,
}

impl<'a, Payments, Receipts, Work, Clock, Telemetry>
    ReconciliationWorker<'a, Payments, Receipts, Work, Clock, Telemetry>
{
    #[must_use]
    pub const fn new(
        payments: &'a Payments,
        receipts: &'a Receipts,
        work: &'a Work,
        clock: &'a Clock,
        telemetry: &'a Telemetry,
        config: ReconciliationWorkerConfig,
    ) -> Self {
        Self {
            payments,
            receipts,
            work,
            clock,
            telemetry,
            config,
        }
    }
}

impl<Payments, Receipts, Work, Clock, Telemetry>
    ReconciliationWorker<'_, Payments, Receipts, Work, Clock, Telemetry>
where
    Payments: PaymentIdempotencyStore,
    Receipts: ReceiptIndexStore,
    Work: ReconciliationWorkStore,
    Clock: ReconciliationClock,
    Telemetry: ReconciliationTelemetry,
{
    /// Runs one bounded, fenced reconciliation cycle. Scheduling the next cycle
    /// and sleeping remain responsibilities of the service host.
    ///
    /// # Errors
    ///
    /// Returns an error for unavailable coordination/clock infrastructure,
    /// clock regression, arithmetic overflow or mismatched network bindings.
    pub fn run_cycle(
        &self,
        owner: WorkerId,
    ) -> Result<ReconciliationCycleOutcome, ReconciliationWorkerError<Work::Error, Clock::Error>>
    {
        self.require_same_network()?;
        let started_at = self.now_after(0)?;
        let lease = match self
            .work
            .acquire_lease(owner, started_at, self.config.lease_duration)
            .map_err(ReconciliationWorkerError::Work)?
        {
            LeaseAcquireOutcome::Acquired(lease) => lease,
            LeaseAcquireOutcome::Busy { expires_at_ms } => {
                self.telemetry
                    .record(ReconciliationTelemetryEvent::LeaseBusy);
                return Ok(ReconciliationCycleOutcome::LeaseBusy { expires_at_ms });
            }
        };
        let jobs = self
            .work
            .due_work(lease, started_at, self.config.page_limit)
            .map_err(ReconciliationWorkerError::Work)?;
        let (lease, report) = self.process_jobs(lease, jobs, started_at)?;
        self.work
            .release_lease(lease)
            .map_err(ReconciliationWorkerError::Work)?;
        self.telemetry
            .record(ReconciliationTelemetryEvent::CycleCompleted(report));
        Ok(ReconciliationCycleOutcome::Processed(report))
    }

    fn process_jobs(
        &self,
        mut lease: ReconciliationLease,
        jobs: Vec<ScheduledReconciliation>,
        mut last_time: u64,
    ) -> Result<
        (ReconciliationLease, ReconciliationCycleReport),
        ReconciliationWorkerError<Work::Error, Clock::Error>,
    > {
        let mut report = ReconciliationCycleReport::default();
        for job in jobs {
            let now = self.now_after(last_time)?;
            last_time = now;
            lease = self
                .work
                .renew_lease(lease, now, self.config.lease_duration)
                .map_err(ReconciliationWorkerError::Work)?;
            report.scanned = increment(report.scanned)?;
            let result = PaymentFinalityReconciler::new(self.payments, self.receipts).reconcile(
                job.request_id,
                job.request_digest,
                now,
            );
            let scheduled_at = self.now_after(last_time)?;
            last_time = scheduled_at;
            match result {
                Ok(
                    PaymentReconciliationOutcome::Finalized(_)
                    | PaymentReconciliationOutcome::AlreadyFinalized(_),
                ) => {
                    self.work
                        .acknowledge_completed(lease, job.request_id, scheduled_at)
                        .map_err(ReconciliationWorkerError::Work)?;
                    report.finalized = increment(report.finalized)?;
                }
                Ok(PaymentReconciliationOutcome::Pending(_)) => {
                    self.defer_pending(lease, job, scheduled_at)?;
                    report.pending = increment(report.pending)?;
                }
                Ok(PaymentReconciliationOutcome::NotSubmitted(_)) => {
                    self.defer_error(lease, job, scheduled_at, &mut report)?;
                }
                Err(_) => self.defer_error(lease, job, scheduled_at, &mut report)?,
            }
        }
        Ok((lease, report))
    }

    fn defer_pending(
        &self,
        lease: ReconciliationLease,
        job: ScheduledReconciliation,
        now_ms: u64,
    ) -> Result<(), ReconciliationWorkerError<Work::Error, Clock::Error>> {
        let next = now_ms
            .checked_add(self.config.retry_policy.pending_delay_ms())
            .ok_or(ReconciliationWorkerError::ArithmeticOverflow)?;
        self.work
            .defer(lease, job, DeferReason::Pending, next, now_ms)
            .map_err(ReconciliationWorkerError::Work)?;
        Ok(())
    }

    fn defer_error(
        &self,
        lease: ReconciliationLease,
        job: ScheduledReconciliation,
        now_ms: u64,
        report: &mut ReconciliationCycleReport,
    ) -> Result<(), ReconciliationWorkerError<Work::Error, Clock::Error>> {
        let failure_number = job
            .consecutive_failures
            .checked_add(1)
            .ok_or(ReconciliationWorkerError::CounterOverflow)?;
        let next = now_ms
            .checked_add(self.config.retry_policy.error_delay_ms(failure_number))
            .ok_or(ReconciliationWorkerError::ArithmeticOverflow)?;
        let deferred = self
            .work
            .defer(lease, job, DeferReason::Error, next, now_ms)
            .map_err(ReconciliationWorkerError::Work)?;
        report.failed = increment(report.failed)?;
        if self
            .config
            .retry_policy
            .should_alert(deferred.consecutive_failures)
        {
            report.alerts = increment(report.alerts)?;
            self.telemetry
                .record(ReconciliationTelemetryEvent::RetryThresholdReached {
                    consecutive_failures: deferred.consecutive_failures,
                });
        }
        Ok(())
    }

    fn require_same_network(
        &self,
    ) -> Result<(), ReconciliationWorkerError<Work::Error, Clock::Error>> {
        let network = self.payments.network();
        if self.receipts.network() == network && self.work.network() == network {
            Ok(())
        } else {
            Err(ReconciliationWorkerError::WrongNetwork)
        }
    }

    fn now_after(
        &self,
        minimum: u64,
    ) -> Result<u64, ReconciliationWorkerError<Work::Error, Clock::Error>> {
        let now = self
            .clock
            .now_ms()
            .map_err(ReconciliationWorkerError::Clock)?;
        if now < minimum {
            Err(ReconciliationWorkerError::ClockRegression)
        } else {
            Ok(now)
        }
    }
}

fn increment<WorkError, ClockError>(
    value: u32,
) -> Result<u32, ReconciliationWorkerError<WorkError, ClockError>> {
    value
        .checked_add(1)
        .ok_or(ReconciliationWorkerError::CounterOverflow)
}
