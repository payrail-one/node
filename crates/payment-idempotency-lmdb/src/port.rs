use ledger_core::{NetworkId, Nonce, OperationReceipt, SignedOperation};
use payment_idempotency_core::{
    IndeterminateReason, PaymentIdempotencyStore, PaymentIntent, PaymentRequestId,
    PaymentReservation, ReconciliationPage, ReconciliationPageLimit, RejectionCode, RequestDigest,
    StoredMutation,
};
use state_sync_core::FinalizedCheckpoint;

use crate::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreError};

impl PaymentIdempotencyStore for LmdbPaymentIdempotencyStore {
    type Error = PaymentIdempotencyStoreError;

    fn network(&self) -> NetworkId {
        self.network()
    }

    fn get(&self, id: PaymentRequestId) -> Result<Option<PaymentReservation>, Self::Error> {
        self.get(id)
    }

    fn scan_reconcilable_after(
        &self,
        after: Option<PaymentRequestId>,
        limit: ReconciliationPageLimit,
    ) -> Result<ReconciliationPage, Self::Error> {
        self.scan_reconcilable_after(after, limit)
    }

    fn reserve(&self, intent: PaymentIntent, now_ms: u64) -> Result<StoredMutation, Self::Error> {
        self.reserve(intent, now_ms)
    }

    fn assign_next_nonce(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        authoritative_next_nonce: Nonce,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.assign_next_nonce(id, digest, authoritative_next_nonce, now_ms)
    }

    fn prepare_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        signed: &SignedOperation,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.prepare_submission(id, digest, signed, now_ms)
    }

    fn mark_indeterminate(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        reason: IndeterminateReason,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.mark_indeterminate(id, digest, reason, now_ms)
    }

    fn mark_published(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.mark_published(id, digest, now_ms)
    }

    fn finalize(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        checkpoint: FinalizedCheckpoint,
        receipt: OperationReceipt,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.finalize(id, digest, checkpoint, receipt, now_ms)
    }

    fn reject_before_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        code: RejectionCode,
        now_ms: u64,
    ) -> Result<StoredMutation, Self::Error> {
        self.reject_before_submission(id, digest, code, now_ms)
    }
}
