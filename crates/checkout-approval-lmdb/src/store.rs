use std::{fs, path::Path};

use checkout_approval_core::{
    ApprovalCodeDigest, ApprovalCodeMutation, ApprovalCodeMutationOutcome, ApprovalCodeRecord,
    ApprovalCodeState,
};
use heed::{Database, Env, RoTxn, RwTxn, types::Bytes};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

use crate::{
    ApprovalCodeStoreError, ApprovalCodeStoreOptions, MINIMUM_MAP_SIZE,
    codec::{account_key, checkout_key, decode_record, digest_key, encode_record},
    environment::open_environment,
};

const NETWORK_KEY: &[u8] = b"network";

#[derive(Debug)]
pub struct LmdbApprovalCodeStore {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) records: Database<Bytes, Bytes>,
    pub(crate) active_accounts: Database<Bytes, Bytes>,
    pub(crate) checkout_owners: Database<Bytes, Bytes>,
    pub(crate) network: NetworkId,
}

impl LmdbApprovalCodeStore {
    /// Opens a network-bound approval-code store with full LMDB durability.
    ///
    /// # Errors
    ///
    /// Rejects unsafe paths, undersized maps, another network, corrupt indexes
    /// and database failures.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: ApprovalCodeStoreOptions,
    ) -> Result<Self, ApprovalCodeStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(ApprovalCodeStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ApprovalCodeStoreError::UnsafePath);
        }
        let canonical = fs::canonicalize(requested)?;
        let env = open_environment(&canonical, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let records = env.create_database(&mut transaction, Some("records"))?;
        let active_accounts = env.create_database(&mut transaction, Some("active_accounts"))?;
        let checkout_owners = env.create_database(&mut transaction, Some("checkout_owners"))?;
        match metadata.get(&transaction, NETWORK_KEY)? {
            Some(stored) if stored != network.as_bytes() => {
                return Err(ApprovalCodeStoreError::WrongNetwork);
            }
            Some(_) => {}
            None => metadata.put(&mut transaction, NETWORK_KEY, network.as_bytes().as_slice())?,
        }
        transaction.commit()?;
        let store = Self {
            env,
            metadata,
            records,
            active_accounts,
            checkout_owners,
            network,
        };
        store.validate_indexes()?;
        Ok(store)
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Atomically publishes a code digest and rotates an unclaimed code for the
    /// same account. A claimed, still-current code cannot be silently replaced.
    ///
    /// # Errors
    ///
    /// Rejects digest collisions, active claims, another network and corrupt
    /// indexes.
    pub fn issue(
        &self,
        record: ApprovalCodeRecord,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, ApprovalCodeStoreError> {
        if record.network() != self.network {
            return Err(ApprovalCodeStoreError::WrongNetwork);
        }
        if !matches!(record.state(), ApprovalCodeState::Ready) {
            return Err(ApprovalCodeStoreError::CorruptRecord);
        }
        let digest = digest_key(record.digest());
        let account = account_key(record.account());
        let mut transaction = self.env.write_txn()?;

        if let Some(existing) = self.read_record(&transaction, record.digest())? {
            if existing == record {
                return Ok(ApprovalCodeMutation {
                    outcome: ApprovalCodeMutationOutcome::ExistingSame,
                    record: existing,
                });
            }
            if !existing.is_expired(now_ms)
                && !matches!(existing.state(), ApprovalCodeState::Consumed { .. })
            {
                return Err(ApprovalCodeStoreError::CodeCollision);
            }
            self.remove_record_indexes(&mut transaction, existing)?;
            self.records.delete(&mut transaction, &digest)?;
        }

        if let Some(previous_digest) = self.active_accounts.get(&transaction, &account)? {
            let previous_digest = decode_digest(previous_digest)?;
            let previous = self
                .read_record(&transaction, previous_digest)?
                .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
            if matches!(previous.state(), ApprovalCodeState::Claimed(_))
                && !previous.is_expired(now_ms)
            {
                return Err(
                    checkout_approval_core::ApprovalCodeError::AccountHasClaimedCode.into(),
                );
            }
            self.remove_record_indexes(&mut transaction, previous)?;
            self.records
                .delete(&mut transaction, &digest_key(previous.digest()))?;
        }

        self.records
            .put(&mut transaction, &digest, &encode_record(record))?;
        self.active_accounts
            .put(&mut transaction, &account, &digest)?;
        transaction.commit()?;
        Ok(ApprovalCodeMutation {
            outcome: ApprovalCodeMutationOutcome::Applied,
            record,
        })
    }

    /// Atomically binds a live code to exactly one checkout.
    ///
    /// # Errors
    ///
    /// Rejects unknown, expired, reused or conflicting codes and checkouts.
    pub fn claim(
        &self,
        digest: ApprovalCodeDigest,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, ApprovalCodeStoreError> {
        let digest_key = digest_key(digest);
        let checkout_key = checkout_key(checkout);
        let mut transaction = self.env.write_txn()?;
        let mut record = self
            .read_record(&transaction, digest)?
            .ok_or(ApprovalCodeStoreError::CodeNotFound)?;
        let account_key = account_key(record.account());
        if self.active_accounts.get(&transaction, &account_key)? != Some(digest_key.as_slice()) {
            return Err(ApprovalCodeStoreError::CorruptRecord);
        }
        if let Some(owner) = self.checkout_owners.get(&transaction, &checkout_key)?
            && owner != digest_key
        {
            return Err(checkout_approval_core::ApprovalCodeError::CheckoutAlreadyLinked.into());
        }
        let outcome = record.claim(checkout, now_ms)?;
        if outcome == ApprovalCodeMutationOutcome::Applied {
            self.records
                .put(&mut transaction, &digest_key, &encode_record(record))?;
            self.checkout_owners
                .put(&mut transaction, &checkout_key, &digest_key)?;
            transaction.commit()?;
        }
        Ok(ApprovalCodeMutation { outcome, record })
    }

    /// Atomically consumes the code after the same payer's signed checkout has
    /// finalized. Consumption after code expiry is allowed because finality can
    /// arrive at the boundary; claiming is never allowed after expiry.
    ///
    /// # Errors
    ///
    /// Rejects missing links, a different payer, invalid state and corruption.
    pub fn consume(
        &self,
        checkout: CheckoutId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutation, ApprovalCodeStoreError> {
        let checkout_key = checkout_key(checkout);
        let mut transaction = self.env.write_txn()?;
        let digest = self
            .checkout_owners
            .get(&transaction, &checkout_key)?
            .ok_or(ApprovalCodeStoreError::CodeNotFound)?;
        let digest = decode_digest(digest)?;
        let mut record = self
            .read_record(&transaction, digest)?
            .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
        if record.account() != account {
            return Err(checkout_approval_core::ApprovalCodeError::AccountMismatch.into());
        }
        let outcome = record.consume(checkout, now_ms)?;
        if outcome == ApprovalCodeMutationOutcome::Applied {
            let digest_key = digest_key(digest);
            self.records
                .put(&mut transaction, &digest_key, &encode_record(record))?;
            let account_key = account_key(account);
            if self.active_accounts.get(&transaction, &account_key)? != Some(digest_key.as_slice())
                || !self
                    .active_accounts
                    .delete(&mut transaction, &account_key)?
            {
                return Err(ApprovalCodeStoreError::CorruptRecord);
            }
            transaction.commit()?;
        }
        Ok(ApprovalCodeMutation { outcome, record })
    }

    /// Reads the approval record linked to a checkout.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt indexes or database failure.
    pub fn by_checkout(
        &self,
        checkout: CheckoutId,
    ) -> Result<Option<ApprovalCodeRecord>, ApprovalCodeStoreError> {
        let transaction = self.env.read_txn()?;
        let Some(digest) = self
            .checkout_owners
            .get(&transaction, &checkout_key(checkout))?
        else {
            return Ok(None);
        };
        let record = self
            .read_record(&transaction, decode_digest(digest)?)?
            .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
        match record.state() {
            ApprovalCodeState::Claimed(claim) | ApprovalCodeState::Consumed { claim, .. }
                if claim.checkout == checkout =>
            {
                Ok(Some(record))
            }
            _ => Err(ApprovalCodeStoreError::CorruptRecord),
        }
    }

    /// Reads a record by an HMAC-authenticated code digest.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt state or an unavailable database.
    pub fn by_digest(
        &self,
        digest: ApprovalCodeDigest,
    ) -> Result<Option<ApprovalCodeRecord>, ApprovalCodeStoreError> {
        let transaction = self.env.read_txn()?;
        self.read_record(&transaction, digest)
    }

    pub(crate) fn read_record(
        &self,
        transaction: &RoTxn<'_>,
        digest: ApprovalCodeDigest,
    ) -> Result<Option<ApprovalCodeRecord>, ApprovalCodeStoreError> {
        let Some(encoded) = self.records.get(transaction, &digest_key(digest))? else {
            return Ok(None);
        };
        let record = decode_record(encoded)?;
        if record.network() != self.network || record.digest() != digest {
            return Err(ApprovalCodeStoreError::CorruptRecord);
        }
        Ok(Some(record))
    }

    fn remove_record_indexes(
        &self,
        transaction: &mut RwTxn<'_>,
        record: ApprovalCodeRecord,
    ) -> Result<(), ApprovalCodeStoreError> {
        let digest = digest_key(record.digest());
        let account = account_key(record.account());
        if self.active_accounts.get(transaction, &account)? == Some(digest.as_slice()) {
            self.active_accounts.delete(transaction, &account)?;
        }
        match record.state() {
            ApprovalCodeState::Ready => {}
            ApprovalCodeState::Claimed(claim) | ApprovalCodeState::Consumed { claim, .. } => {
                let checkout = checkout_key(claim.checkout);
                if self.checkout_owners.get(transaction, &checkout)? == Some(digest.as_slice()) {
                    self.checkout_owners.delete(transaction, &checkout)?;
                }
            }
        }
        Ok(())
    }
}

fn decode_digest(input: &[u8]) -> Result<ApprovalCodeDigest, ApprovalCodeStoreError> {
    let value: [u8; 32] = input
        .try_into()
        .map_err(|_| ApprovalCodeStoreError::CorruptRecord)?;
    Ok(ApprovalCodeDigest::new(value))
}
