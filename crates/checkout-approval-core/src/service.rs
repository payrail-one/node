use ledger_core::AccountId;
use merchant_checkout_core::CheckoutId;

use crate::{
    ApprovalCode, ApprovalCodeAuthenticator, ApprovalCodeMutation, ApprovalCodePolicy,
    ApprovalCodeRecord, ApprovalCodeServiceError, ApprovalCodeStore, IssuedApprovalCode,
};

#[derive(Clone, Copy, Debug)]
pub struct ApprovalCodeService<'a, Authenticator, Store> {
    authenticator: &'a Authenticator,
    store: &'a Store,
    policy: ApprovalCodePolicy,
}

impl<'a, Authenticator, Store> ApprovalCodeService<'a, Authenticator, Store> {
    #[must_use]
    pub const fn new(
        authenticator: &'a Authenticator,
        store: &'a Store,
        policy: ApprovalCodePolicy,
    ) -> Self {
        Self {
            authenticator,
            store,
            policy,
        }
    }
}

impl<Authenticator, Store> ApprovalCodeService<'_, Authenticator, Store>
where
    Authenticator: ApprovalCodeAuthenticator,
    Store: ApprovalCodeStore,
{
    /// Issues a code for a payer account. Plaintext is returned once and is not
    /// passed to persistence.
    ///
    /// # Errors
    ///
    /// Rejects invalid policy, a digest collision or store failure.
    pub fn issue(
        &self,
        account: AccountId,
        code: ApprovalCode,
        now_ms: u64,
    ) -> Result<IssuedApprovalCode, ApprovalCodeServiceError<Store::Error>> {
        let network = self.store.network();
        let digest = self.authenticator.digest(network, code);
        let record = ApprovalCodeRecord::issue(network, digest, account, self.policy, now_ms)?;
        let stored = self
            .store
            .issue(record, now_ms)
            .map_err(ApprovalCodeServiceError::Store)?;
        if stored.record != record {
            return Err(ApprovalCodeServiceError::CodeCollision);
        }
        Ok(IssuedApprovalCode {
            code: code.expose(),
            account,
            expires_at_ms: record.expires_at_ms(),
        })
    }

    /// Claims the payer code for one merchant checkout.
    ///
    /// # Errors
    ///
    /// Rejects malformed, expired, unknown, reused or conflicting codes.
    pub fn claim(
        &self,
        code: &str,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, ApprovalCodeServiceError<Store::Error>> {
        let code = ApprovalCode::parse(code)?;
        let digest = self.authenticator.digest(self.store.network(), code);
        self.store
            .claim(digest, checkout, now_ms)
            .map_err(ApprovalCodeServiceError::Store)
    }

    /// Consumes a claimed code only after the same account's signed checkout
    /// payment finalizes.
    ///
    /// # Errors
    ///
    /// Rejects account/checkout conflicts, invalid state or store failures.
    pub fn consume(
        &self,
        checkout: CheckoutId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, ApprovalCodeServiceError<Store::Error>> {
        self.store
            .consume(checkout, account, now_ms)
            .map_err(ApprovalCodeServiceError::Store)
    }

    /// Reads the approval record linked to a checkout.
    ///
    /// # Errors
    ///
    /// Propagates persistent store failure.
    pub fn by_checkout(
        &self,
        checkout: CheckoutId,
    ) -> Result<Option<ApprovalCodeRecord>, ApprovalCodeServiceError<Store::Error>> {
        self.store
            .by_checkout(checkout)
            .map_err(ApprovalCodeServiceError::Store)
    }

    /// Reads one code record through an already authenticated opaque-session
    /// digest. Plaintext approval codes must never be accepted here.
    ///
    /// # Errors
    ///
    /// Propagates persistent store failure.
    pub fn by_digest(
        &self,
        digest: crate::ApprovalCodeDigest,
    ) -> Result<Option<ApprovalCodeRecord>, ApprovalCodeServiceError<Store::Error>> {
        self.store
            .by_digest(digest)
            .map_err(ApprovalCodeServiceError::Store)
    }
}
