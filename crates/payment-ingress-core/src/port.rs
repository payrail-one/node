use ledger_core::AccountId;
use payment_gateway_core::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentGateway, PaymentGatewayError,
    PaymentGatewayOutcome, PaymentOperationSigner, PaymentPolicy, PaymentPublisher,
    SignedPaymentVerifier,
};
use payment_idempotency_core::{PaymentIdempotencyStore, TenantId};
use payment_idempotency_core::{PaymentRequestId, PaymentReservation};

use crate::{ApiPrincipalId, RateLimitDecision};

pub trait PaymentIngressJournal {
    type Error;

    fn network(&self) -> ledger_core::NetworkId;

    /// Reads one durable request for ingress classification.
    ///
    /// # Errors
    ///
    /// Returns the journal error when the record cannot be read safely.
    fn get(&self, id: PaymentRequestId) -> Result<Option<PaymentReservation>, Self::Error>;
}

impl<Store: PaymentIdempotencyStore> PaymentIngressJournal for Store {
    type Error = Store::Error;

    fn network(&self) -> ledger_core::NetworkId {
        PaymentIdempotencyStore::network(self)
    }

    fn get(&self, id: PaymentRequestId) -> Result<Option<PaymentReservation>, Self::Error> {
        PaymentIdempotencyStore::get(self, id)
    }
}

pub trait PaymentIngressRateLimiter {
    type Error;

    /// Charges one authenticated API request before any journal lookup.
    ///
    /// # Errors
    ///
    /// Returns an implementation error when time or bounded accounting cannot
    /// be advanced safely.
    fn admit_principal(
        &mut self,
        principal: ApiPrincipalId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error>;

    /// Atomically charges tenant and account quota for a journal-confirmed new
    /// payment intent.
    ///
    /// # Errors
    ///
    /// Returns an implementation error when time or bounded accounting cannot
    /// be advanced safely.
    fn admit_new_intent(
        &mut self,
        tenant: TenantId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error>;
}

pub trait PaymentSubmissionGateway {
    type Error;

    /// Submits a canonical request through durable idempotency and publication.
    ///
    /// # Errors
    ///
    /// Returns the underlying gateway error.
    fn submit(
        &self,
        request: CanonicalPaymentRequest,
        now_ms: u64,
    ) -> Result<PaymentGatewayOutcome, Self::Error>;
}

impl<Store, Nonces, Policy, Signer, Verifier, Publisher> PaymentSubmissionGateway
    for PaymentGateway<'_, Store, Nonces, Policy, Signer, Verifier, Publisher>
where
    Store: PaymentIdempotencyStore,
    Nonces: AccountNonceSource,
    Policy: PaymentPolicy<Error = Nonces::Error>,
    Signer: PaymentOperationSigner<Error = Nonces::Error>,
    Verifier: SignedPaymentVerifier<Error = Nonces::Error>,
    Publisher: PaymentPublisher<Error = Nonces::Error>,
{
    type Error = PaymentGatewayError<Store::Error, Nonces::Error>;

    fn submit(
        &self,
        request: CanonicalPaymentRequest,
        now_ms: u64,
    ) -> Result<PaymentGatewayOutcome, Self::Error> {
        PaymentGateway::submit(self, request, now_ms)
    }
}
