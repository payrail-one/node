use checkout_approval_core::ApprovalCodeState;

use crate::{
    ApprovalCodeStoreError,
    codec::{account_key, checkout_key, decode_record, digest_key},
    store::LmdbApprovalCodeStore,
};

impl LmdbApprovalCodeStore {
    pub(crate) fn validate_indexes(&self) -> Result<(), ApprovalCodeStoreError> {
        let transaction = self.env.read_txn()?;
        if self.metadata.get(&transaction, b"network")? != Some(self.network.as_bytes()) {
            return Err(ApprovalCodeStoreError::WrongNetwork);
        }
        let mut active_count = 0_u64;
        let mut checkout_count = 0_u64;
        for item in self.records.iter(&transaction)? {
            let (key, encoded) = item?;
            let record = decode_record(encoded)?;
            let digest = digest_key(record.digest());
            if key != digest || record.network() != self.network {
                return Err(ApprovalCodeStoreError::CorruptRecord);
            }
            let active = self
                .active_accounts
                .get(&transaction, &account_key(record.account()))?;
            match record.state() {
                ApprovalCodeState::Ready => {
                    require_owner(active, &digest)?;
                    active_count = increment(active_count)?;
                }
                ApprovalCodeState::Claimed(claim) => {
                    require_owner(active, &digest)?;
                    require_owner(
                        self.checkout_owners
                            .get(&transaction, &checkout_key(claim.checkout))?,
                        &digest,
                    )?;
                    active_count = increment(active_count)?;
                    checkout_count = increment(checkout_count)?;
                }
                ApprovalCodeState::Consumed { claim, .. } => {
                    if active.is_some() {
                        return Err(ApprovalCodeStoreError::CorruptRecord);
                    }
                    require_owner(
                        self.checkout_owners
                            .get(&transaction, &checkout_key(claim.checkout))?,
                        &digest,
                    )?;
                    checkout_count = increment(checkout_count)?;
                }
            }
        }
        if self.active_accounts.len(&transaction)? != active_count
            || self.checkout_owners.len(&transaction)? != checkout_count
        {
            return Err(ApprovalCodeStoreError::CorruptRecord);
        }
        Ok(())
    }
}

fn require_owner(owner: Option<&[u8]>, digest: &[u8; 32]) -> Result<(), ApprovalCodeStoreError> {
    if owner == Some(digest.as_slice()) {
        Ok(())
    } else {
        Err(ApprovalCodeStoreError::CorruptRecord)
    }
}

fn increment(value: u64) -> Result<u64, ApprovalCodeStoreError> {
    value
        .checked_add(1)
        .ok_or(ApprovalCodeStoreError::CorruptRecord)
}
