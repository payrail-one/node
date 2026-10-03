#![forbid(unsafe_code)]

use payment_idempotency_core::{
    PaymentIdempotencyStore, PaymentRequestId, PaymentReservation, ReconciliationPageLimit,
    RequestDigest, ReservationState, StoredMutation,
};
use receipt_index_core::ReceiptIndexStore;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentReconciliationError<PaymentError, ReceiptError> {
    Payment(PaymentError),
    Receipt(ReceiptError),
    RequestNotFound,
    WrongNetwork,
    RequestConflict,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentReconciliationOutcome {
    NotSubmitted(PaymentReservation),
    Pending(PaymentReservation),
    Finalized(StoredMutation),
    AlreadyFinalized(PaymentReservation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentReconciliationBatchError<PaymentError> {
    Payment(PaymentError),
    WrongNetwork,
}

pub type PaymentReconciliationItemResult<PaymentError, ReceiptError> =
    Result<PaymentReconciliationOutcome, PaymentReconciliationError<PaymentError, ReceiptError>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentReconciliationBatchItem<PaymentError, ReceiptError> {
    pub request_id: PaymentRequestId,
    pub result: PaymentReconciliationItemResult<PaymentError, ReceiptError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentReconciliationBatch<PaymentError, ReceiptError> {
    pub items: Vec<PaymentReconciliationBatchItem<PaymentError, ReceiptError>>,
    pub next_cursor: Option<PaymentRequestId>,
}

pub type PaymentReconciliationBatchResult<PaymentError, ReceiptError> = Result<
    PaymentReconciliationBatch<PaymentError, ReceiptError>,
    PaymentReconciliationBatchError<PaymentError>,
>;

#[derive(Clone, Copy, Debug)]
pub struct PaymentFinalityReconciler<'a, Payments, Receipts> {
    payments: &'a Payments,
    receipts: &'a Receipts,
}

impl<'a, Payments, Receipts> PaymentFinalityReconciler<'a, Payments, Receipts> {
    #[must_use]
    pub const fn new(payments: &'a Payments, receipts: &'a Receipts) -> Self {
        Self { payments, receipts }
    }
}

impl<Payments, Receipts> PaymentFinalityReconciler<'_, Payments, Receipts>
where
    Payments: PaymentIdempotencyStore,
    Receipts: ReceiptIndexStore,
{
    /// Reconciles one bounded page while isolating failures to individual
    /// requests. A caller can persist `next_cursor` during a sweep and reset it
    /// to `None` when the sweep completes.
    ///
    /// # Errors
    ///
    /// Returns an error only when the payment queue cannot be scanned or the
    /// two stores are bound to different networks. Per-request payment and
    /// receipt errors are retained in the corresponding batch item.
    pub fn reconcile_batch(
        &self,
        after: Option<PaymentRequestId>,
        limit: ReconciliationPageLimit,
        now_ms: u64,
    ) -> PaymentReconciliationBatchResult<Payments::Error, Receipts::Error> {
        if self.payments.network() != self.receipts.network() {
            return Err(PaymentReconciliationBatchError::WrongNetwork);
        }
        let page = self
            .payments
            .scan_reconcilable_after(after, limit)
            .map_err(PaymentReconciliationBatchError::Payment)?;
        let items = page
            .reservations
            .into_iter()
            .map(|reservation| {
                let intent = reservation.intent();
                PaymentReconciliationBatchItem {
                    request_id: intent.request_id,
                    result: self.reconcile(intent.request_id, intent.request_digest, now_ms),
                }
            })
            .collect();
        Ok(PaymentReconciliationBatch {
            items,
            next_cursor: page.next_cursor,
        })
    }

    /// Reconciles one durable client request against independently verified
    /// finalized receipt evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for store failures, a network mismatch, a missing
    /// request or a caller digest that does not match the immutable intent.
    pub fn reconcile(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<
        PaymentReconciliationOutcome,
        PaymentReconciliationError<Payments::Error, Receipts::Error>,
    > {
        if self.payments.network() != self.receipts.network() {
            return Err(PaymentReconciliationError::WrongNetwork);
        }
        let reservation = self
            .payments
            .get(id)
            .map_err(PaymentReconciliationError::Payment)?
            .ok_or(PaymentReconciliationError::RequestNotFound)?;
        if reservation.intent().request_digest != digest {
            return Err(PaymentReconciliationError::RequestConflict);
        }
        let operation_id = match reservation.state() {
            ReservationState::Reserved
            | ReservationState::NonceAssigned(_)
            | ReservationState::Rejected { .. } => {
                return Ok(PaymentReconciliationOutcome::NotSubmitted(reservation));
            }
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission)
            | ReservationState::Indeterminate { submission, .. } => submission.operation_id,
            ReservationState::Finalized(_) => {
                return Ok(PaymentReconciliationOutcome::AlreadyFinalized(reservation));
            }
        };
        let Some(indexed) = self
            .receipts
            .finalized_receipt_by_operation_id(operation_id)
            .map_err(PaymentReconciliationError::Receipt)?
        else {
            return Ok(PaymentReconciliationOutcome::Pending(reservation));
        };
        let finalized = self
            .payments
            .finalize(id, digest, indexed.checkpoint, indexed.receipt, now_ms)
            .map_err(PaymentReconciliationError::Payment)?;
        Ok(PaymentReconciliationOutcome::Finalized(finalized))
    }
}
