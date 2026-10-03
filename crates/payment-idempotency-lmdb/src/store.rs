use std::{fs, path::Path};

use heed::{
    Database, Env, RoTxn, RwTxn,
    types::{Bytes, Unit},
};
use ledger_core::{AccountId, NetworkId, Nonce, OperationReceipt, SignedOperation};
use payment_idempotency_core::{
    IndeterminateReason, MutationOutcome, PaymentIdempotencyError, PaymentIntent, PaymentRequestId,
    PaymentReservation, RejectionCode, RequestDigest, StoredMutation,
};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    MINIMUM_MAP_SIZE, PaymentIdempotencyStoreError, PaymentIdempotencyStoreOptions,
    codec::{account_nonce_key, decode_record, encode_record, request_key},
    environment::open_environment,
};

const NETWORK_KEY: &[u8] = b"network";

#[derive(Debug)]
pub struct LmdbPaymentIdempotencyStore {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) records: Database<Bytes, Bytes>,
    pub(crate) account_next_nonces: Database<Bytes, Bytes>,
    pub(crate) nonce_owners: Database<Bytes, Bytes>,
    pub(crate) operation_owners: Database<Bytes, Bytes>,
    pub(crate) reconciliation_queue: Database<Bytes, Bytes>,
    pub(crate) reconciliation_due: Database<Bytes, Unit>,
    pub(crate) network: NetworkId,
}

impl LmdbPaymentIdempotencyStore {
    /// Opens a network-bound payment request journal with LMDB's safe default
    /// durability and locking flags.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths, invalid options, another network,
    /// corrupt indexes or an LMDB failure.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: PaymentIdempotencyStoreOptions,
    ) -> Result<Self, PaymentIdempotencyStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(PaymentIdempotencyStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(PaymentIdempotencyStoreError::UnsafePath);
        }
        let path = fs::canonicalize(requested)?;
        let env = open_environment(&path, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let records = env.create_database(&mut transaction, Some("records"))?;
        let account_next_nonces =
            env.create_database(&mut transaction, Some("account_next_nonces"))?;
        let nonce_owners = env.create_database(&mut transaction, Some("nonce_owners"))?;
        let operation_owners = env.create_database(&mut transaction, Some("operation_owners"))?;
        let reconciliation_queue =
            env.create_database(&mut transaction, Some("reconciliation_queue"))?;
        let reconciliation_due =
            env.create_database(&mut transaction, Some("reconciliation_due"))?;
        match metadata.get(&transaction, NETWORK_KEY)? {
            Some(stored) if stored != network.as_bytes() => {
                return Err(PaymentIdempotencyStoreError::WrongNetwork);
            }
            Some(_) => {}
            None => metadata.put(&mut transaction, NETWORK_KEY, network.as_bytes().as_slice())?,
        }
        transaction.commit()?;
        let store = Self {
            env,
            metadata,
            records,
            account_next_nonces,
            nonce_owners,
            operation_owners,
            reconciliation_queue,
            reconciliation_due,
            network,
        };
        store.initialize_reconciliation_queue()?;
        store.validate_indexes()?;
        Ok(store)
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Returns the complete durable state for a client request.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt storage or an LMDB failure.
    pub fn get(
        &self,
        id: PaymentRequestId,
    ) -> Result<Option<PaymentReservation>, PaymentIdempotencyStoreError> {
        let transaction = self.env.read_txn()?;
        self.read_record(&transaction, id)
    }

    /// Creates an immutable request reservation before nonce allocation.
    ///
    /// Exact retries return the existing reservation. Reusing the request key
    /// with any different digest or ledger correlation key fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error for another network, a conflicting request or storage
    /// failure.
    pub fn reserve(
        &self,
        intent: PaymentIntent,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        if intent.network != self.network {
            return Err(PaymentIdempotencyStoreError::WrongNetwork);
        }
        let key = request_key(intent.request_id);
        let mut transaction = self.env.write_txn()?;
        if let Some(existing) = self.read_record(&transaction, intent.request_id)? {
            if existing.intent() != intent {
                return Err(PaymentIdempotencyError::RequestConflict.into());
            }
            return Ok(StoredMutation {
                outcome: MutationOutcome::ExistingSame,
                reservation: existing,
            });
        }
        let reservation = PaymentReservation::new(intent, now_ms);
        self.put_record(&mut transaction, &key, &reservation)?;
        transaction.commit()?;
        Ok(StoredMutation {
            outcome: MutationOutcome::Applied,
            reservation,
        })
    }

    /// Atomically allocates the next globally unique nonce for the account.
    ///
    /// The authoritative ledger nonce raises the local floor. Reserved nonces
    /// are never recycled automatically, including after rejection.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing/conflicting request, exhausted nonce space,
    /// corrupt indexes or storage failure.
    pub fn assign_next_nonce(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        authoritative_next_nonce: Nonce,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        let key = request_key(id);
        let mut transaction = self.env.write_txn()?;
        let mut reservation = self.required_record(&transaction, id)?;
        if let Some(existing) = reservation.state().nonce() {
            let outcome = reservation.assign_nonce(digest, existing, now_ms)?;
            return Ok(StoredMutation {
                outcome,
                reservation,
            });
        }
        let stored_next = self.read_account_next_nonce(&transaction, id.account)?;
        let nonce = stored_next.map_or(authoritative_next_nonce, |stored| {
            stored.max(authoritative_next_nonce)
        });
        let next = nonce
            .checked_add(1)
            .ok_or(PaymentIdempotencyError::ArithmeticOverflow)?;
        let nonce_key = account_nonce_key(id.account, nonce);
        if self.nonce_owners.get(&transaction, &nonce_key)?.is_some() {
            return Err(PaymentIdempotencyStoreError::DuplicateNonce);
        }
        let outcome = reservation.assign_nonce(digest, nonce, now_ms)?;
        self.nonce_owners.put(&mut transaction, &nonce_key, &key)?;
        self.account_next_nonces.put(
            &mut transaction,
            id.account.as_bytes().as_slice(),
            &next.to_be_bytes(),
        )?;
        self.put_record(&mut transaction, &key, &reservation)?;
        transaction.commit()?;
        Ok(StoredMutation {
            outcome,
            reservation,
        })
    }

    /// Journals the exact canonical signed envelope before publication.
    ///
    /// # Errors
    ///
    /// Returns an error if the signed operation conflicts with the reservation,
    /// another request owns its operation ID, or storage fails.
    pub fn prepare_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        signed: &SignedOperation,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        let key = request_key(id);
        let mut transaction = self.env.write_txn()?;
        let mut reservation = self.required_record(&transaction, id)?;
        let outcome = reservation.prepare_submission(digest, signed, now_ms)?;
        if outcome == MutationOutcome::Applied {
            let operation_id = reservation
                .state()
                .operation_id()
                .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            match self
                .operation_owners
                .get(&transaction, operation_id.as_bytes().as_slice())?
            {
                Some(owner) if owner != key => {
                    return Err(PaymentIdempotencyStoreError::DuplicateOperation);
                }
                Some(_) => {}
                None => self.operation_owners.put(
                    &mut transaction,
                    operation_id.as_bytes().as_slice(),
                    &key,
                )?,
            }
            self.sync_reconciliation_queue(&mut transaction, &key, &reservation)?;
            self.put_record(&mut transaction, &key, &reservation)?;
            transaction.commit()?;
        }
        Ok(StoredMutation {
            outcome,
            reservation,
        })
    }

    /// Persists an unknown publication outcome without replacing the envelope.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing request, invalid transition or LMDB
    /// failure.
    pub fn mark_indeterminate(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        reason: IndeterminateReason,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        self.mutate(id, |reservation| {
            reservation.mark_indeterminate(digest, reason, now_ms)
        })
    }

    /// Persists a positive publication acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing request, invalid transition or LMDB
    /// failure.
    pub fn mark_published(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        self.mutate(id, |reservation| reservation.mark_published(digest, now_ms))
    }

    /// Atomically attaches matching finalized receipt evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing request, mismatched evidence, invalid
    /// transition or LMDB failure.
    pub fn finalize(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        checkpoint: FinalizedCheckpoint,
        receipt: OperationReceipt,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        self.mutate(id, |reservation| {
            reservation.finalize(digest, checkpoint, receipt, now_ms)
        })
    }

    /// Rejects a request while it is still safe to do so before nonce allocation.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing request, conflict, invalid transition or
    /// LMDB failure.
    pub fn reject_before_submission(
        &self,
        id: PaymentRequestId,
        digest: RequestDigest,
        code: RejectionCode,
        now_ms: u64,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError> {
        self.mutate(id, |reservation| {
            reservation.reject_before_submission(digest, code, now_ms)
        })
    }

    fn mutate<F>(
        &self,
        id: PaymentRequestId,
        transition: F,
    ) -> Result<StoredMutation, PaymentIdempotencyStoreError>
    where
        F: FnOnce(&mut PaymentReservation) -> Result<MutationOutcome, PaymentIdempotencyError>,
    {
        let key = request_key(id);
        let mut transaction = self.env.write_txn()?;
        let mut reservation = self.required_record(&transaction, id)?;
        let outcome = transition(&mut reservation)?;
        if outcome == MutationOutcome::Applied {
            self.sync_reconciliation_queue(&mut transaction, &key, &reservation)?;
            self.put_record(&mut transaction, &key, &reservation)?;
            transaction.commit()?;
        }
        Ok(StoredMutation {
            outcome,
            reservation,
        })
    }

    pub(crate) fn read_record(
        &self,
        transaction: &RoTxn<'_>,
        id: PaymentRequestId,
    ) -> Result<Option<PaymentReservation>, PaymentIdempotencyStoreError> {
        let key = request_key(id);
        let Some(encoded) = self.records.get(transaction, &key)? else {
            return Ok(None);
        };
        let reservation = decode_record(encoded)?;
        if reservation.intent().network != self.network || reservation.intent().request_id != id {
            return Err(PaymentIdempotencyStoreError::CorruptRecord);
        }
        Ok(Some(reservation))
    }

    fn required_record(
        &self,
        transaction: &RoTxn<'_>,
        id: PaymentRequestId,
    ) -> Result<PaymentReservation, PaymentIdempotencyStoreError> {
        self.read_record(transaction, id)?
            .ok_or(PaymentIdempotencyStoreError::RequestNotFound)
    }

    fn put_record(
        &self,
        transaction: &mut RwTxn<'_>,
        key: &[u8],
        reservation: &PaymentReservation,
    ) -> Result<(), PaymentIdempotencyStoreError> {
        let encoded = encode_record(reservation)?;
        self.records.put(transaction, key, encoded.as_slice())?;
        Ok(())
    }

    fn read_account_next_nonce(
        &self,
        transaction: &RoTxn<'_>,
        account: AccountId,
    ) -> Result<Option<Nonce>, PaymentIdempotencyStoreError> {
        self.account_next_nonces
            .get(transaction, account.as_bytes().as_slice())?
            .map(|encoded| {
                encoded
                    .try_into()
                    .map(u64::from_be_bytes)
                    .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)
            })
            .transpose()
    }
}
