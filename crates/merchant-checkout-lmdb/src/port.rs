use ledger_core::{AccountId, Balance, NetworkId};
use merchant_checkout_core::{
    Checkout, CheckoutDefinition, CheckoutId, CheckoutMutation, MerchantCheckoutStore,
    PendingCheckoutPage,
};
use payment_idempotency_core::{ReconciliationPageLimit, TenantId};

use crate::{LmdbMerchantCheckoutStore, MerchantCheckoutStoreError};

impl MerchantCheckoutStore for LmdbMerchantCheckoutStore {
    type Error = MerchantCheckoutStoreError;

    fn network(&self) -> NetworkId {
        self.network()
    }

    fn create(
        &self,
        definition: CheckoutDefinition,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error> {
        self.create(definition, now_ms)
    }

    fn get(&self, tenant: TenantId, id: CheckoutId) -> Result<Option<Checkout>, Self::Error> {
        self.get(tenant, id)
    }

    fn claim(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        payer: AccountId,
        fee: Balance,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error> {
        self.claim(tenant, id, payer, fee, now_ms)
    }

    fn mark_gateway_recorded(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        now_ms: u64,
    ) -> Result<CheckoutMutation, Self::Error> {
        self.mark_gateway_recorded(tenant, id, now_ms)
    }

    fn pending_dispatch_after(
        &self,
        after: Option<CheckoutId>,
        limit: ReconciliationPageLimit,
    ) -> Result<PendingCheckoutPage, Self::Error> {
        self.pending_dispatch_after(after, limit)
    }
}
