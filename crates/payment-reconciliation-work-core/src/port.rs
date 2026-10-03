use ledger_core::NetworkId;
use payment_idempotency_core::{PaymentRequestId, ReconciliationPageLimit};

use crate::{
    DeferReason, LeaseAcquireOutcome, LeaseDuration, ReconciliationLease, ScheduledReconciliation,
    WorkerId,
};

pub trait ReconciliationWorkStore {
    type Error;

    fn network(&self) -> NetworkId;

    /// Acquires a new fenced lease only when no unexpired owner exists.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when coordination state is corrupt or cannot
    /// be persisted.
    fn acquire_lease(
        &self,
        owner: WorkerId,
        now_ms: u64,
        duration: LeaseDuration,
    ) -> Result<LeaseAcquireOutcome, Self::Error>;

    /// Extends an active lease without changing its fencing token.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the lease expired, was replaced or could
    /// not be persisted.
    fn renew_lease(
        &self,
        lease: ReconciliationLease,
        now_ms: u64,
        duration: LeaseDuration,
    ) -> Result<ReconciliationLease, Self::Error>;

    /// Releases the exact fenced lease. A stale owner cannot release a newer
    /// owner's lease.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for stale ownership or persistence failure.
    fn release_lease(&self, lease: ReconciliationLease) -> Result<(), Self::Error>;

    /// Returns the earliest due jobs while validating the active fenced lease.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for stale ownership, corrupt indexes or an
    /// unavailable store.
    fn due_work(
        &self,
        lease: ReconciliationLease,
        now_ms: u64,
        limit: ReconciliationPageLimit,
    ) -> Result<Vec<ScheduledReconciliation>, Self::Error>;

    /// Atomically replaces a job's due index and retry state under the active
    /// fenced lease.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for stale ownership, changed work, invalid
    /// time, overflow, corruption or persistence failure.
    fn defer(
        &self,
        lease: ReconciliationLease,
        job: ScheduledReconciliation,
        reason: DeferReason,
        next_attempt_at_ms: u64,
        now_ms: u64,
    ) -> Result<ScheduledReconciliation, Self::Error>;

    /// Removes a stale scheduled row that no longer has a reconcilable journal
    /// state. This is idempotent if concurrent finalization already removed it.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for stale lease ownership or persistence
    /// failure.
    fn acknowledge_completed(
        &self,
        lease: ReconciliationLease,
        request_id: PaymentRequestId,
        now_ms: u64,
    ) -> Result<(), Self::Error>;
}
