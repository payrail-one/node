use checkout_approval_core::{
    ApprovalCodeDigest, ApprovalCodeMutation, ApprovalCodeRecord, ApprovalCodeStore,
};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

use crate::{ApprovalCodeStoreError, LmdbApprovalCodeStore};

impl ApprovalCodeStore for LmdbApprovalCodeStore {
    type Error = ApprovalCodeStoreError;

    fn network(&self) -> NetworkId {
        self.network()
    }

    fn issue(
        &self,
        record: ApprovalCodeRecord,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error> {
        self.issue(record, now_ms)
    }

    fn claim(
        &self,
        digest: ApprovalCodeDigest,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error> {
        self.claim(digest, checkout, now_ms)
    }

    fn consume(
        &self,
        checkout: CheckoutId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, Self::Error> {
        self.consume(checkout, account, now_ms)
    }

    fn by_checkout(&self, checkout: CheckoutId) -> Result<Option<ApprovalCodeRecord>, Self::Error> {
        self.by_checkout(checkout)
    }

    fn by_digest(
        &self,
        digest: ApprovalCodeDigest,
    ) -> Result<Option<ApprovalCodeRecord>, Self::Error> {
        self.by_digest(digest)
    }
}
