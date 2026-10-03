use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

use crate::{ApprovalCode, ApprovalCodeDigest, ApprovalCodeMutation, ApprovalCodeRecord};

pub trait ApprovalCodeAuthenticator {
    fn digest(&self, network: NetworkId, code: ApprovalCode) -> ApprovalCodeDigest;
}

pub trait ApprovalCodeGenerator {
    type Error;

    /// Produces a fresh candidate code.
    ///
    /// # Errors
    ///
    /// Returns an adapter-specific error when secure randomness is unavailable.
    fn generate(&self) -> Result<ApprovalCode, Self::Error>;
}

pub trait ApprovalCodeStore {
    type Error;

    fn network(&self) -> NetworkId;

    /// Persists a newly issued digest and its account binding.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error for a collision, conflict,
    /// corruption or persistence failure.
    fn issue(
        &self,
        record: ApprovalCodeRecord,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error>;

    /// Claims a current digest for one checkout.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error for missing, expired, reused or
    /// conflicting state and persistence failure.
    fn claim(
        &self,
        digest: ApprovalCodeDigest,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error>;

    /// Consumes a checkout link after signed payment finality.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error for payer mismatch, invalid
    /// state, corruption or persistence failure.
    fn consume(
        &self,
        checkout: CheckoutId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error>;

    /// Retrieves the record linked to a checkout.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence or corruption error.
    fn by_checkout(&self, checkout: CheckoutId) -> Result<Option<ApprovalCodeRecord>, Self::Error>;

    /// Retrieves a record by an authenticated digest for an opaque issuer
    /// session.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific persistence or corruption error.
    fn by_digest(
        &self,
        digest: ApprovalCodeDigest,
    ) -> Result<Option<ApprovalCodeRecord>, Self::Error>;
}
