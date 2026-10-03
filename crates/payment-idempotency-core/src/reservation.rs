use ledger_core::{Nonce, OperationReceipt, SignedOperation};
use state_sync_core::FinalizedCheckpoint;
use transaction_protocol::SignedOperationCodec;

use crate::{
    FinalizedPayment, IndeterminateReason, MutationOutcome, PaymentIdempotencyError, PaymentIntent,
    PreparedSubmission, RejectionCode, RequestDigest, ReservationState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentReservation {
    intent: PaymentIntent,
    created_at_ms: u64,
    updated_at_ms: u64,
    state: ReservationState,
}

impl PaymentReservation {
    #[must_use]
    pub const fn new(intent: PaymentIntent, now_ms: u64) -> Self {
        Self {
            intent,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            state: ReservationState::Reserved,
        }
    }

    /// Restores persisted state through the same invariant checks used by live
    /// transitions.
    ///
    /// # Errors
    ///
    /// Returns an error for regressing timestamps or inconsistent signed and
    /// finalized operation data.
    pub fn restore(
        intent: PaymentIntent,
        created_at_ms: u64,
        updated_at_ms: u64,
        state: ReservationState,
    ) -> Result<Self, PaymentIdempotencyError> {
        if updated_at_ms < created_at_ms {
            return Err(PaymentIdempotencyError::TimestampRegression);
        }
        let reservation = Self {
            intent,
            created_at_ms,
            updated_at_ms,
            state,
        };
        reservation.validate_state()?;
        Ok(reservation)
    }

    #[must_use]
    pub const fn intent(&self) -> PaymentIntent {
        self.intent
    }

    #[must_use]
    pub const fn created_at_ms(&self) -> u64 {
        self.created_at_ms
    }

    #[must_use]
    pub const fn updated_at_ms(&self) -> u64 {
        self.updated_at_ms
    }

    #[must_use]
    pub const fn state(&self) -> &ReservationState {
        &self.state
    }

    /// Assigns the durable account nonce exactly once.
    ///
    /// # Errors
    ///
    /// Returns an error when the request conflicts, time regresses or a
    /// different nonce was already assigned.
    pub fn assign_nonce(
        &mut self,
        digest: RequestDigest,
        nonce: Nonce,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        match self.state {
            ReservationState::Reserved => {
                self.state = ReservationState::NonceAssigned(nonce);
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::NonceAssigned(existing) if existing == nonce => {
                Ok(MutationOutcome::ExistingSame)
            }
            ReservationState::NonceAssigned(_) => Err(PaymentIdempotencyError::NonceMismatch),
            _ => self.existing_nonce_outcome(nonce),
        }
    }

    /// Persists the exact signed bytes before any network publication.
    ///
    /// # Errors
    ///
    /// Returns an error unless the operation matches the reserved network,
    /// sender, idempotency key and nonce, or conflicts with prior bytes.
    pub fn prepare_submission(
        &mut self,
        digest: RequestDigest,
        signed: &SignedOperation,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        let submission = self.submission_from_signed(signed)?;
        match &self.state {
            ReservationState::NonceAssigned(nonce) if *nonce == submission.nonce => {
                self.state = ReservationState::SubmissionPrepared(submission);
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::NonceAssigned(_) => Err(PaymentIdempotencyError::NonceMismatch),
            ReservationState::SubmissionPrepared(existing)
            | ReservationState::Published(existing)
            | ReservationState::Indeterminate {
                submission: existing,
                ..
            } if *existing == submission => Ok(MutationOutcome::ExistingSame),
            ReservationState::Finalized(finalized) if finalized.submission == submission => {
                Ok(MutationOutcome::ExistingSame)
            }
            ReservationState::SubmissionPrepared(_)
            | ReservationState::Published(_)
            | ReservationState::Indeterminate { .. }
            | ReservationState::Finalized(_) => Err(PaymentIdempotencyError::OperationMismatch),
            ReservationState::Reserved | ReservationState::Rejected { .. } => {
                Err(PaymentIdempotencyError::InvalidTransition)
            }
        }
    }

    /// Marks a journaled submission for explicit reconciliation without
    /// allowing a replacement transaction to be created.
    ///
    /// # Errors
    ///
    /// Returns an error for a conflicting or premature transition.
    pub fn mark_indeterminate(
        &mut self,
        digest: RequestDigest,
        reason: IndeterminateReason,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        match &self.state {
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission) => {
                self.state = ReservationState::Indeterminate {
                    submission: submission.clone(),
                    reason,
                };
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::Indeterminate {
                reason: existing, ..
            } if *existing == reason => Ok(MutationOutcome::ExistingSame),
            ReservationState::Finalized(_) => Ok(MutationOutcome::ExistingSame),
            _ => Err(PaymentIdempotencyError::InvalidTransition),
        }
    }

    /// Records a positive publication acknowledgement for the exact envelope.
    ///
    /// # Errors
    ///
    /// Returns an error before preparation or after terminal rejection.
    pub fn mark_published(
        &mut self,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        match &self.state {
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Indeterminate { submission, .. } => {
                self.state = ReservationState::Published(submission.clone());
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::Published(_) | ReservationState::Finalized(_) => {
                Ok(MutationOutcome::ExistingSame)
            }
            _ => Err(PaymentIdempotencyError::InvalidTransition),
        }
    }

    /// Binds the prepared operation to its independently indexed final receipt.
    ///
    /// # Errors
    ///
    /// Returns an error when finality does not describe the exact journaled
    /// operation or when the transition is invalid.
    pub fn finalize(
        &mut self,
        digest: RequestDigest,
        checkpoint: FinalizedCheckpoint,
        receipt: OperationReceipt,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        match &self.state {
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission)
            | ReservationState::Indeterminate { submission, .. } => {
                self.validate_receipt(submission, receipt)?;
                self.state = ReservationState::Finalized(Box::new(FinalizedPayment {
                    submission: submission.clone(),
                    checkpoint,
                    receipt,
                }));
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::Finalized(existing)
                if existing.checkpoint == checkpoint && existing.receipt == receipt =>
            {
                Ok(MutationOutcome::ExistingSame)
            }
            ReservationState::Finalized(_) => Err(PaymentIdempotencyError::ReceiptMismatch),
            _ => Err(PaymentIdempotencyError::InvalidTransition),
        }
    }

    /// Rejects an intent only while no signed envelope can exist.
    ///
    /// Rejection is allowed only before nonce allocation. Once allocated, the
    /// request must be recovered and signed rather than abandoned, so a strict
    /// account nonce sequence cannot be permanently wedged by a terminal gap.
    ///
    /// # Errors
    ///
    /// Returns an error after submission preparation or for a conflicting retry.
    pub fn reject_before_submission(
        &mut self,
        digest: RequestDigest,
        code: RejectionCode,
        now_ms: u64,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        self.require_request(digest, now_ms)?;
        match self.state {
            ReservationState::Reserved => {
                self.state = ReservationState::Rejected { code };
                self.updated_at_ms = now_ms;
                Ok(MutationOutcome::Applied)
            }
            ReservationState::Rejected { code: existing, .. } if existing == code => {
                Ok(MutationOutcome::ExistingSame)
            }
            ReservationState::Rejected { .. } => Err(PaymentIdempotencyError::RequestConflict),
            _ => Err(PaymentIdempotencyError::InvalidTransition),
        }
    }

    fn require_request(
        &self,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<(), PaymentIdempotencyError> {
        if digest != self.intent.request_digest {
            return Err(PaymentIdempotencyError::RequestConflict);
        }
        if now_ms < self.updated_at_ms {
            return Err(PaymentIdempotencyError::TimestampRegression);
        }
        Ok(())
    }

    fn submission_from_signed(
        &self,
        signed: &SignedOperation,
    ) -> Result<PreparedSubmission, PaymentIdempotencyError> {
        let operation = &signed.operation;
        if operation.network() != self.intent.network {
            return Err(PaymentIdempotencyError::WrongNetwork);
        }
        if operation.sender() != self.intent.request_id.account
            || operation.idempotency_key() != self.intent.ledger_idempotency_key
        {
            return Err(PaymentIdempotencyError::OperationMismatch);
        }
        let expected_nonce = self
            .state
            .nonce()
            .ok_or(PaymentIdempotencyError::InvalidTransition)?;
        if operation.nonce() != expected_nonce {
            return Err(PaymentIdempotencyError::NonceMismatch);
        }
        let operation_id = operation
            .operation_id()
            .map_err(|_| PaymentIdempotencyError::InvalidEnvelope)?;
        let envelope = SignedOperationCodec::encode(signed)
            .map_err(|_| PaymentIdempotencyError::InvalidEnvelope)?;
        Ok(PreparedSubmission {
            nonce: expected_nonce,
            operation_id,
            envelope,
        })
    }

    fn validate_submission(
        &self,
        submission: &PreparedSubmission,
    ) -> Result<(), PaymentIdempotencyError> {
        let signed = SignedOperationCodec::decode(&submission.envelope)
            .map_err(|_| PaymentIdempotencyError::InvalidEnvelope)?;
        let expected = self.submission_from_signed(&signed)?;
        if expected == *submission {
            Ok(())
        } else {
            Err(PaymentIdempotencyError::OperationMismatch)
        }
    }

    fn validate_receipt(
        &self,
        submission: &PreparedSubmission,
        receipt: OperationReceipt,
    ) -> Result<(), PaymentIdempotencyError> {
        if receipt.operation_id == submission.operation_id
            && receipt.account == self.intent.request_id.account
            && receipt.idempotency_key == self.intent.ledger_idempotency_key
            && receipt.nonce == submission.nonce
        {
            Ok(())
        } else {
            Err(PaymentIdempotencyError::ReceiptMismatch)
        }
    }

    fn validate_state(&self) -> Result<(), PaymentIdempotencyError> {
        match &self.state {
            ReservationState::Reserved
            | ReservationState::NonceAssigned(_)
            | ReservationState::Rejected { .. } => Ok(()),
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission)
            | ReservationState::Indeterminate { submission, .. } => {
                self.validate_submission(submission)
            }
            ReservationState::Finalized(finalized) => {
                self.validate_submission(&finalized.submission)?;
                self.validate_receipt(&finalized.submission, finalized.receipt)
            }
        }
    }

    fn existing_nonce_outcome(
        &self,
        nonce: Nonce,
    ) -> Result<MutationOutcome, PaymentIdempotencyError> {
        match self.state.nonce() {
            Some(existing) if existing == nonce => Ok(MutationOutcome::ExistingSame),
            Some(_) => Err(PaymentIdempotencyError::NonceMismatch),
            None => Err(PaymentIdempotencyError::InvalidTransition),
        }
    }
}
