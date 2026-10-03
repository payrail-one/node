use ledger_core::{AccountId, Balance};
use payment_gateway_core::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentGateway, PaymentGatewayError,
    PaymentGatewayOutcome, PaymentOperationSigner, PaymentPolicy, PaymentPublisher,
    SignedPaymentVerifier,
};
use payment_idempotency_core::{
    PaymentIdempotencyStore, ReconciliationPageLimit, ReservationState, TenantId,
};

use crate::{
    Checkout, CheckoutDefinition, CheckoutDispatchState, CheckoutId, CheckoutMutation,
    CheckoutPaymentGateway, CheckoutPaymentStatus, CheckoutServiceError, CheckoutState, FeeMode,
    MerchantCheckoutStore,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutPaymentOutcome {
    pub checkout: Checkout,
    pub gateway: PaymentGatewayOutcome,
}

pub type CheckoutServiceResult<Value, CheckoutStoreError, PaymentStoreError, GatewayError> =
    Result<Value, CheckoutServiceError<CheckoutStoreError, PaymentStoreError, GatewayError>>;

pub type CheckoutDispatchResult<CheckoutStoreError, PaymentStoreError, GatewayError> =
    CheckoutServiceResult<
        CheckoutPaymentOutcome,
        CheckoutStoreError,
        PaymentStoreError,
        GatewayError,
    >;

pub type CheckoutDispatchBatchResult<CheckoutStoreError, PaymentStoreError, GatewayError> =
    CheckoutServiceResult<
        CheckoutDispatchBatch<CheckoutStoreError, PaymentStoreError, GatewayError>,
        CheckoutStoreError,
        PaymentStoreError,
        GatewayError,
    >;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutDispatchItem<CheckoutStoreError, PaymentStoreError, GatewayError> {
    pub checkout_id: CheckoutId,
    pub result: CheckoutDispatchResult<CheckoutStoreError, PaymentStoreError, GatewayError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutDispatchBatch<CheckoutStoreError, PaymentStoreError, GatewayError> {
    pub items: Vec<CheckoutDispatchItem<CheckoutStoreError, PaymentStoreError, GatewayError>>,
    pub next_cursor: Option<CheckoutId>,
}

#[derive(Clone, Copy, Debug)]
pub struct MerchantCheckoutService<'a, Checkouts, Payments, Gateway> {
    checkouts: &'a Checkouts,
    payments: &'a Payments,
    gateway: &'a Gateway,
}

impl<'a, Checkouts, Payments, Gateway> MerchantCheckoutService<'a, Checkouts, Payments, Gateway> {
    #[must_use]
    pub const fn new(
        checkouts: &'a Checkouts,
        payments: &'a Payments,
        gateway: &'a Gateway,
    ) -> Self {
        Self {
            checkouts,
            payments,
            gateway,
        }
    }
}

impl<Checkouts, Payments, Gateway> MerchantCheckoutService<'_, Checkouts, Payments, Gateway>
where
    Checkouts: MerchantCheckoutStore,
    Payments: PaymentIdempotencyStore,
    Gateway: CheckoutPaymentGateway,
{
    /// Creates or returns the same immutable merchant checkout.
    ///
    /// # Errors
    ///
    /// Returns an error for a network mismatch, definition conflict or store
    /// failure.
    pub fn create(
        &self,
        definition: CheckoutDefinition,
        now_ms: u64,
    ) -> CheckoutServiceResult<CheckoutMutation, Checkouts::Error, Payments::Error, Gateway::Error>
    {
        self.require_network(definition.network)?;
        self.checkouts
            .create(definition, now_ms)
            .map_err(CheckoutServiceError::Checkout)
    }

    /// Atomically claims a checkout and attempts exact-idempotent gateway
    /// submission. A gateway failure leaves a durable pending-dispatch row.
    ///
    /// # Errors
    ///
    /// Returns a typed checkout, payment-journal or gateway error.
    pub fn pay(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        payer: AccountId,
        fee: Balance,
        now_ms: u64,
    ) -> CheckoutDispatchResult<Checkouts::Error, Payments::Error, Gateway::Error> {
        self.require_network(self.checkouts.network())?;
        let claimed = self
            .checkouts
            .claim(tenant, id, payer, fee, now_ms)
            .map_err(CheckoutServiceError::Checkout)?;
        self.dispatch_checkout(&claimed.checkout, now_ms)
    }

    /// Derives the merchant-visible status without treating publication as
    /// irreversible finality.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing checkout, network mismatch or store
    /// failure.
    pub fn status(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        now_ms: u64,
    ) -> CheckoutServiceResult<
        CheckoutPaymentStatus,
        Checkouts::Error,
        Payments::Error,
        Gateway::Error,
    > {
        self.require_network(self.checkouts.network())?;
        let checkout = self
            .checkouts
            .get(tenant, id)
            .map_err(CheckoutServiceError::Checkout)?
            .ok_or(CheckoutServiceError::CheckoutNotFound)?;
        match checkout.state() {
            CheckoutState::Open if now_ms >= checkout.definition().expires_at_ms => {
                Ok(CheckoutPaymentStatus::Expired)
            }
            CheckoutState::Open => Ok(CheckoutPaymentStatus::Open),
            CheckoutState::Claimed { claim, .. } => {
                let request_id = claim.payment_request_id(checkout.definition().tenant);
                let reservation = self
                    .payments
                    .get(request_id)
                    .map_err(CheckoutServiceError::Payment)?;
                Ok(
                    reservation.map_or(CheckoutPaymentStatus::PaymentStarting, |stored| {
                        payment_status(stored.state())
                    }),
                )
            }
        }
    }

    /// Retries one bounded page of crash-recoverable checkout dispatch work.
    /// Item failures do not prevent later items in the same page from running.
    ///
    /// # Errors
    ///
    /// Returns an error only when the pending-dispatch page itself cannot be
    /// read or network bindings conflict. Per-item errors stay in the batch.
    pub fn dispatch_pending(
        &self,
        after: Option<CheckoutId>,
        limit: ReconciliationPageLimit,
        now_ms: u64,
    ) -> CheckoutDispatchBatchResult<Checkouts::Error, Payments::Error, Gateway::Error> {
        self.require_network(self.checkouts.network())?;
        let page = self
            .checkouts
            .pending_dispatch_after(after, limit)
            .map_err(CheckoutServiceError::Checkout)?;
        let items = page
            .checkouts
            .into_iter()
            .map(|checkout| CheckoutDispatchItem {
                checkout_id: checkout.id(),
                result: self.dispatch_checkout(&checkout, now_ms),
            })
            .collect();
        Ok(CheckoutDispatchBatch {
            items,
            next_cursor: page.next_cursor,
        })
    }

    fn dispatch_checkout(
        &self,
        checkout: &Checkout,
        now_ms: u64,
    ) -> CheckoutDispatchResult<Checkouts::Error, Payments::Error, Gateway::Error> {
        let CheckoutState::Claimed { claim, dispatch } = checkout.state() else {
            return Err(CheckoutServiceError::CheckoutNotFound);
        };
        if dispatch == CheckoutDispatchState::GatewayRecorded {
            let request_id = claim.payment_request_id(checkout.definition().tenant);
            let stored = self
                .payments
                .get(request_id)
                .map_err(CheckoutServiceError::Payment)?
                .ok_or(CheckoutServiceError::PaymentJournalMissing)?;
            return Ok(CheckoutPaymentOutcome {
                checkout: *checkout,
                gateway: outcome_from_reservation(stored)?,
            });
        }
        let definition = checkout.definition();
        let request = CanonicalPaymentRequest {
            network: definition.network,
            request_id: claim.payment_request_id(definition.tenant),
            asset: definition.asset,
            recipient: definition.merchant_account,
            amount: definition.amount,
            fee: claim.fee,
            fee_payer: match definition.fee_mode {
                FeeMode::CustomerPays => None,
                FeeMode::MerchantSponsored => Some(definition.merchant_account),
            },
            valid_until_height: definition.valid_until_height,
        }
        .validate()
        .map_err(CheckoutServiceError::InvalidPayment)?;
        let gateway = self
            .gateway
            .submit(request, now_ms)
            .map_err(CheckoutServiceError::Gateway)?;
        let recorded = self
            .checkouts
            .mark_gateway_recorded(definition.tenant, checkout.id(), now_ms)
            .map_err(CheckoutServiceError::Checkout)?;
        Ok(CheckoutPaymentOutcome {
            checkout: recorded.checkout,
            gateway,
        })
    }

    fn require_network(
        &self,
        network: ledger_core::NetworkId,
    ) -> CheckoutServiceResult<(), Checkouts::Error, Payments::Error, Gateway::Error> {
        if network == self.payments.network() && network == self.checkouts.network() {
            Ok(())
        } else {
            Err(CheckoutServiceError::WrongNetwork)
        }
    }
}

impl<Store, Nonces, Policy, Signer, Verifier, Publisher> CheckoutPaymentGateway
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

fn payment_status(state: &ReservationState) -> CheckoutPaymentStatus {
    match state {
        ReservationState::Reserved | ReservationState::NonceAssigned(_) => {
            CheckoutPaymentStatus::Processing
        }
        ReservationState::SubmissionPrepared(_) => CheckoutPaymentStatus::Processing,
        ReservationState::Indeterminate { .. } => CheckoutPaymentStatus::OutcomeUnknown,
        ReservationState::Published(_) => CheckoutPaymentStatus::Accepted,
        ReservationState::Finalized(finalized) => match finalized.receipt.outcome {
            ledger_core::OperationOutcome::Applied => CheckoutPaymentStatus::Finalized,
            ledger_core::OperationOutcome::Expired => CheckoutPaymentStatus::PaymentExpired,
        },
        ReservationState::Rejected { .. } => CheckoutPaymentStatus::Rejected,
    }
}

fn outcome_from_reservation<CheckoutStoreError, PaymentStoreError, GatewayError>(
    reservation: payment_idempotency_core::PaymentReservation,
) -> Result<
    PaymentGatewayOutcome,
    CheckoutServiceError<CheckoutStoreError, PaymentStoreError, GatewayError>,
> {
    match reservation.state() {
        ReservationState::Finalized(finalized)
            if finalized.receipt.outcome == ledger_core::OperationOutcome::Expired =>
        {
            Ok(PaymentGatewayOutcome::Expired(reservation))
        }
        ReservationState::Finalized(_) => Ok(PaymentGatewayOutcome::Finalized(reservation)),
        ReservationState::Rejected { .. } => Ok(PaymentGatewayOutcome::Rejected(reservation)),
        ReservationState::SubmissionPrepared(_)
        | ReservationState::Published(_)
        | ReservationState::Indeterminate { .. } => {
            Ok(PaymentGatewayOutcome::Submitted(reservation))
        }
        ReservationState::Reserved | ReservationState::NonceAssigned(_) => {
            Err(CheckoutServiceError::PaymentJournalMissing)
        }
    }
}
