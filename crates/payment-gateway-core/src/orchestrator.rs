use ledger_core::{OperationOutcome, SignedOperation};
use payment_idempotency_core::{
    IndeterminateReason, PaymentIdempotencyStore, PaymentReservation, RejectionCode,
    ReservationState,
};
use transaction_protocol::SignedOperationCodec;

use crate::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentOperationSigner, PaymentPolicy,
    PaymentPublisher, PaymentRequestError, SignedPaymentVerifier,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentServiceStage {
    NonceRead,
    Policy,
    Signing,
    Verification,
    Publication,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentGatewayError<StoreError, ServiceError> {
    InvalidRequest(PaymentRequestError),
    Store(StoreError),
    Service {
        stage: PaymentServiceStage,
        source: ServiceError,
    },
    CorruptJournal,
    SignedOperationMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaymentGatewayOutcome {
    Submitted(PaymentReservation),
    Finalized(PaymentReservation),
    Expired(PaymentReservation),
    Rejected(PaymentReservation),
}

#[derive(Clone, Copy, Debug)]
pub struct PaymentGateway<'a, Store, Nonces, Policy, Signer, Verifier, Publisher> {
    store: &'a Store,
    nonces: &'a Nonces,
    policy: &'a Policy,
    signer: &'a Signer,
    verifier: &'a Verifier,
    publisher: &'a Publisher,
}

impl<'a, Store, Nonces, Policy, Signer, Verifier, Publisher>
    PaymentGateway<'a, Store, Nonces, Policy, Signer, Verifier, Publisher>
{
    #[must_use]
    pub const fn new(
        store: &'a Store,
        nonces: &'a Nonces,
        policy: &'a Policy,
        signer: &'a Signer,
        verifier: &'a Verifier,
        publisher: &'a Publisher,
    ) -> Self {
        Self {
            store,
            nonces,
            policy,
            signer,
            verifier,
            publisher,
        }
    }
}

impl<Store, Nonces, Policy, Signer, Verifier, Publisher, ServiceError>
    PaymentGateway<'_, Store, Nonces, Policy, Signer, Verifier, Publisher>
where
    Store: PaymentIdempotencyStore,
    Nonces: AccountNonceSource<Error = ServiceError>,
    Policy: PaymentPolicy<Error = ServiceError>,
    Signer: PaymentOperationSigner<Error = ServiceError>,
    Verifier: SignedPaymentVerifier<Error = ServiceError>,
    Publisher: PaymentPublisher<Error = ServiceError>,
{
    /// Safely drives one payment through policy, nonce allocation, signing,
    /// durable journaling and publication. Exact retries reuse the journaled
    /// envelope and never call the signer again.
    ///
    /// # Errors
    ///
    /// Returns a typed request, store or service-stage error. A publication
    /// error is returned only after the durable request is marked indeterminate.
    pub fn submit(
        &self,
        request: CanonicalPaymentRequest,
        now_ms: u64,
    ) -> Result<PaymentGatewayOutcome, PaymentGatewayError<Store::Error, ServiceError>> {
        let intent = request
            .intent()
            .map_err(PaymentGatewayError::InvalidRequest)?;
        let reserved = self
            .store
            .reserve(intent, now_ms)
            .map_err(PaymentGatewayError::Store)?;
        match reserved.reservation.state() {
            ReservationState::Finalized(_) => {
                return Ok(finalized_outcome(reserved.reservation));
            }
            ReservationState::Rejected { .. } => {
                return Ok(PaymentGatewayOutcome::Rejected(reserved.reservation));
            }
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission)
            | ReservationState::Indeterminate { submission, .. } => {
                let signed = SignedOperationCodec::decode(&submission.envelope)
                    .map_err(|_| PaymentGatewayError::CorruptJournal)?;
                Self::require_request_operation(&request, submission.nonce, &signed)?;
                return self.publish(&request, &signed, now_ms);
            }
            ReservationState::Reserved => self.authorize_or_reject(&request, intent, now_ms)?,
            ReservationState::NonceAssigned(_) => {}
        }

        let current = self
            .store
            .get(request.request_id)
            .map_err(PaymentGatewayError::Store)?
            .ok_or(PaymentGatewayError::CorruptJournal)?;
        let nonce = match current.state() {
            ReservationState::NonceAssigned(nonce) => *nonce,
            ReservationState::Reserved => {
                let authoritative = self
                    .nonces
                    .finalized_next_nonce(request.network, request.request_id.account)
                    .map_err(|source| PaymentGatewayError::Service {
                        stage: PaymentServiceStage::NonceRead,
                        source,
                    })?;
                let assigned = self
                    .store
                    .assign_next_nonce(
                        request.request_id,
                        intent.request_digest,
                        authoritative,
                        now_ms,
                    )
                    .map_err(PaymentGatewayError::Store)?;
                assigned
                    .reservation
                    .state()
                    .nonce()
                    .ok_or(PaymentGatewayError::CorruptJournal)?
            }
            ReservationState::SubmissionPrepared(submission)
            | ReservationState::Published(submission)
            | ReservationState::Indeterminate { submission, .. } => {
                let signed = SignedOperationCodec::decode(&submission.envelope)
                    .map_err(|_| PaymentGatewayError::CorruptJournal)?;
                Self::require_request_operation(&request, submission.nonce, &signed)?;
                return self.publish(&request, &signed, now_ms);
            }
            ReservationState::Finalized(_) => {
                return Ok(finalized_outcome(current));
            }
            ReservationState::Rejected { .. } => {
                return Ok(PaymentGatewayOutcome::Rejected(current));
            }
        };
        let operation = request
            .operation(nonce)
            .map_err(PaymentGatewayError::InvalidRequest)?;
        let signed =
            self.signer
                .sign(operation.clone())
                .map_err(|source| PaymentGatewayError::Service {
                    stage: PaymentServiceStage::Signing,
                    source,
                })?;
        if signed.operation != operation {
            return Err(PaymentGatewayError::SignedOperationMismatch);
        }
        self.verifier
            .verify(&signed)
            .map_err(|source| PaymentGatewayError::Service {
                stage: PaymentServiceStage::Verification,
                source,
            })?;
        self.store
            .prepare_submission(request.request_id, intent.request_digest, &signed, now_ms)
            .map_err(PaymentGatewayError::Store)?;
        self.publish(&request, &signed, now_ms)
    }

    fn authorize_or_reject(
        &self,
        request: &CanonicalPaymentRequest,
        intent: payment_idempotency_core::PaymentIntent,
        now_ms: u64,
    ) -> Result<(), PaymentGatewayError<Store::Error, ServiceError>> {
        if let Err(source) = self.policy.authorize(request) {
            self.store
                .reject_before_submission(
                    request.request_id,
                    intent.request_digest,
                    RejectionCode::PolicyDenied,
                    now_ms,
                )
                .map_err(PaymentGatewayError::Store)?;
            return Err(PaymentGatewayError::Service {
                stage: PaymentServiceStage::Policy,
                source,
            });
        }
        Ok(())
    }

    fn publish(
        &self,
        request: &CanonicalPaymentRequest,
        signed: &SignedOperation,
        now_ms: u64,
    ) -> Result<PaymentGatewayOutcome, PaymentGatewayError<Store::Error, ServiceError>> {
        if let Err(source) = self.publisher.publish(signed) {
            self.store
                .mark_indeterminate(
                    request.request_id,
                    request.request_digest(),
                    IndeterminateReason::BroadcastOutcomeUnknown,
                    now_ms,
                )
                .map_err(PaymentGatewayError::Store)?;
            return Err(PaymentGatewayError::Service {
                stage: PaymentServiceStage::Publication,
                source,
            });
        }
        let published = self
            .store
            .mark_published(request.request_id, request.request_digest(), now_ms)
            .map_err(PaymentGatewayError::Store)?;
        Ok(PaymentGatewayOutcome::Submitted(published.reservation))
    }

    fn require_request_operation(
        request: &CanonicalPaymentRequest,
        nonce: u64,
        signed: &SignedOperation,
    ) -> Result<(), PaymentGatewayError<Store::Error, ServiceError>> {
        let expected = (*request)
            .operation(nonce)
            .map_err(PaymentGatewayError::InvalidRequest)?;
        if signed.operation == expected {
            Ok(())
        } else {
            Err(PaymentGatewayError::SignedOperationMismatch)
        }
    }
}

fn finalized_outcome(reservation: PaymentReservation) -> PaymentGatewayOutcome {
    let expired = matches!(
        reservation.state(),
        ReservationState::Finalized(finalized)
            if finalized.receipt.outcome == OperationOutcome::Expired
    );
    if expired {
        PaymentGatewayOutcome::Expired(reservation)
    } else {
        PaymentGatewayOutcome::Finalized(reservation)
    }
}
