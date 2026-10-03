use ledger_core::{NetworkId, Nonce, OperationReceipt, SignedOperation};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    IndeterminateReason, PaymentIntent, PaymentRequestId, PaymentReservation, ReconciliationPage,
    ReconciliationPageLimit, RejectionCode, RequestDigest, StoredMutation,
};

pub trait PaymentIdempotencyStore {
    type Error;

    /// Returns the immutable network binding of this journal.
    fn network(&self) -> NetworkId;

    /// Reads one durable client request.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when storage is unavailable or corrupt.
    fn get(&self, id: PaymentRequestId) -> Result<Option<PaymentReservation>, Self::Error>;

    /// Reads one stable, ordered page of submissions that still need finality
    /// reconciliation. The cursor is exclusive.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when storage is unavailable or corrupt.
    fn scan_reconcilable_after(
        &self,
        after: Option<PaymentRequestId>,
        limit: ReconciliationPageLimit,
    ) -> Result<ReconciliationPage, Self::Error>;

    /// Reserves an immutable request before nonce allocation.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for a conflict or persistence failure.
    fn reserve(&self, intent: PaymentIntent, now_ms: u64) -> Result<StoredMutation, Self::Error>;

    /// Atomically allocates or returns this request's account nonce.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for invalid state, exhausted nonce space or a
    /// persistence failure.
    fn assign_next_nonce(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        authoritative_next_nonce: Nonce,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;

    /// Journals the exact signed envelope before network publication.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the envelope conflicts with the request
    /// or cannot be persisted.
    fn prepare_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        signed: &SignedOperation,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;

    /// Records that a prepared submission needs explicit reconciliation.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for invalid state, conflict or persistence
    /// failure.
    fn mark_indeterminate(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        reason: IndeterminateReason,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;

    /// Records a positive publication acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for invalid state, conflict or persistence
    /// failure.
    fn mark_published(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;

    /// Advances a prepared request using finalized receipt evidence.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when evidence mismatches or persistence fails.
    fn finalize(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        checkpoint: FinalizedCheckpoint,
        receipt: OperationReceipt,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;

    /// Rejects a request before any nonce has been allocated.
    ///
    /// # Errors
    ///
    /// Returns the adapter error after nonce allocation, for a conflict or when
    /// persistence fails.
    fn reject_before_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        code: RejectionCode,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error>;
}
