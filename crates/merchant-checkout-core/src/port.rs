use ledger_core::{AccountId, Balance, NetworkId};
use payment_gateway_core::{CanonicalPaymentRequest, PaymentGatewayOutcome};
use payment_idempotency_core::{ReconciliationPageLimit, TenantId};

use crate::{Checkout, CheckoutDefinition, CheckoutId, CheckoutMutation, PendingCheckoutPage};

pub trait MerchantCheckoutStore {
    type Error;

    fn network(&self) -> NetworkId;

    /// Creates an immutable checkout or returns its exact prior value.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for invalid input, conflict or persistence
    /// failure.
    fn create(
        &self,
        definition: CheckoutDefinition,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error>;

    /// Reads a checkout only within its owning tenant.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when storage is unavailable or corrupt.
    fn get(&self, tenant: TenantId, id: CheckoutId) -> Result<Option<Checkout>, Self::Error>;

    /// Claims an open checkout for one immutable payer and fee selection.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for expiry, conflict, invalid policy or
    /// persistence failure.
    fn claim(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        payer: AccountId,
        fee: Balance,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error>;

    /// Records that the gateway journal durably represents this payment.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for invalid state or persistence failure.
    fn mark_gateway_recorded(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error>;

    /// Reads a bounded stable page of claims requiring gateway dispatch.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when storage is unavailable or corrupt.
    fn pending_dispatch_after(
        &self,
        after: Option<CheckoutId>,
        limit: ReconciliationPageLimit,
    ) -> Result<PendingCheckoutPage, Self::Error>;
}

pub trait CheckoutPaymentGateway {
    type Error;

    /// Submits an exact canonical payment through the gateway journal.
    ///
    /// # Errors
    ///
    /// Returns the gateway error while leaving its own durable recovery state
    /// authoritative.
    fn submit(
        &self,
        request: CanonicalPaymentRequest,
        now_ms: u64,
    ) -> Result<PaymentGatewayOutcome, Self::Error>;
}
