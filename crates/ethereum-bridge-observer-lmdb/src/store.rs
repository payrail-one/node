use std::{fs, path::Path};

use ethereum_bridge_core::{EvmObserverCheckpoint, EvmObserverScope, PreparedEvmObserverBlock};
use external_asset_core::DepositId;
use heed::{Database, Env, EnvOpenOptions, types::Bytes};

use crate::{
    EvmObserverCommitOutcome, EvmObserverStoreError, EvmObserverStoreOptions,
    MAX_DEPOSIT_PAGE_SIZE, MINIMUM_MAP_SIZE, StoredEvmDeposit,
    codec::{
        block_digest, decode_checkpoint, decode_deposit, decode_last_block, decode_scope,
        encode_checkpoint, encode_deposit, encode_last_block, encode_scope,
    },
    types::{LastBlockRecord, StoreState},
};

const SCOPE_KEY: &[u8] = b"scope";
const CHECKPOINT_KEY: &[u8] = b"checkpoint";
const LAST_BLOCK_KEY: &[u8] = b"last_block";
const MAX_DATABASES: u32 = 4;

#[derive(Debug)]
pub struct LmdbEvmObserverStore {
    env: Env,
    metadata: Database<Bytes, Bytes>,
    deposits: Database<Bytes, Bytes>,
    scope: EvmObserverScope,
}

impl LmdbEvmObserverStore {
    /// Opens or initializes a restart-safe observer journal bound to one chain,
    /// bridge contract and trusted starting parent.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe path, invalid options, scope mismatch,
    /// corrupt journal or LMDB failure.
    pub fn open(
        path: impl AsRef<Path>,
        scope: EvmObserverScope,
        options: EvmObserverStoreOptions,
    ) -> Result<Self, EvmObserverStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(EvmObserverStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(EvmObserverStoreError::UnsafePath);
        }
        let canonical = fs::canonicalize(requested)?;
        let env = open_environment(&canonical, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let deposits = env.create_database(&mut transaction, Some("deposits"))?;
        let encoded_scope = encode_scope(scope);
        match metadata.get(&transaction, SCOPE_KEY)? {
            Some(stored) if decode_scope(stored)? != scope => {
                return Err(EvmObserverStoreError::WrongScope);
            }
            Some(_) => {}
            None => {
                if metadata.get(&transaction, CHECKPOINT_KEY)?.is_some()
                    || metadata.get(&transaction, LAST_BLOCK_KEY)?.is_some()
                    || deposits.len(&transaction)? != 0
                {
                    return Err(EvmObserverStoreError::CorruptRecord);
                }
                metadata.put(&mut transaction, SCOPE_KEY, encoded_scope.as_slice())?;
                let checkpoint = encode_checkpoint(scope.initial_checkpoint());
                metadata.put(&mut transaction, CHECKPOINT_KEY, checkpoint.as_slice())?;
            }
        }
        transaction.commit()?;
        let store = Self {
            env,
            metadata,
            deposits,
            scope,
        };
        store.validate_state()?;
        Ok(store)
    }

    #[must_use]
    pub const fn scope(&self) -> EvmObserverScope {
        self.scope
    }

    /// Returns the next expected block and its required canonical parent hash.
    ///
    /// # Errors
    ///
    /// Returns an error when the durable cursor is missing or corrupt.
    pub fn checkpoint(&self) -> Result<EvmObserverCheckpoint, EvmObserverStoreError> {
        let transaction = self.env.read_txn()?;
        self.read_state(&transaction).map(|state| state.checkpoint)
    }

    /// Atomically records every verified deposit in one finalized block and
    /// advances the observer cursor. Exact retries are idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error for another scope, a gap/fork/stale cursor, conflicting
    /// block/deposit, corrupt state or LMDB failure. No partial write is visible.
    pub fn commit(
        &self,
        prepared: &PreparedEvmObserverBlock,
    ) -> Result<EvmObserverCommitOutcome, EvmObserverStoreError> {
        if prepared.scope() != self.scope {
            return Err(EvmObserverStoreError::WrongScope);
        }
        let digest = block_digest(prepared);
        let requested_last = LastBlockRecord {
            number: prepared.finalized().number,
            hash: prepared.finalized().hash,
            digest,
        };
        let mut transaction = self.env.write_txn()?;
        let state = self.read_state(&transaction)?;
        if state.checkpoint == prepared.next() {
            if state.last_block != Some(requested_last) {
                return Err(EvmObserverStoreError::ConflictingBlock);
            }
            self.require_same_deposits(&transaction, prepared)?;
            return Ok(EvmObserverCommitOutcome::ExistingSame);
        }
        if state.checkpoint.next_block == prepared.next().next_block {
            return Err(EvmObserverStoreError::ConflictingBlock);
        }
        if state.checkpoint != prepared.previous() {
            return Err(EvmObserverStoreError::CursorMismatch);
        }
        self.require_new_deposits(&transaction, prepared)?;
        for deposit in prepared.deposits() {
            let id = DepositId::derive(&deposit.claim());
            let encoded = encode_deposit(deposit);
            self.deposits.put(
                &mut transaction,
                id.as_bytes().as_slice(),
                encoded.as_slice(),
            )?;
        }
        let encoded_last = encode_last_block(requested_last);
        self.metadata
            .put(&mut transaction, LAST_BLOCK_KEY, encoded_last.as_slice())?;
        let encoded_checkpoint = encode_checkpoint(prepared.next());
        self.metadata.put(
            &mut transaction,
            CHECKPOINT_KEY,
            encoded_checkpoint.as_slice(),
        )?;
        transaction.commit()?;
        Ok(EvmObserverCommitOutcome::Committed)
    }

    /// Returns one stored verified deposit by its canonical origin-event ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the record is corrupt or LMDB cannot read it.
    pub fn deposit(
        &self,
        id: DepositId,
    ) -> Result<Option<StoredEvmDeposit>, EvmObserverStoreError> {
        let transaction = self.env.read_txn()?;
        self.deposits
            .get(&transaction, id.as_bytes().as_slice())?
            .map(|bytes| decode_deposit(id, bytes))
            .transpose()
    }

    /// Reads a bounded deterministic page ordered by deposit ID.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero/oversized limit, corrupt record or LMDB
    /// failure.
    pub fn deposits_after(
        &self,
        after: Option<DepositId>,
        limit: usize,
    ) -> Result<Vec<StoredEvmDeposit>, EvmObserverStoreError> {
        if !(1..=MAX_DEPOSIT_PAGE_SIZE).contains(&limit) {
            return Err(EvmObserverStoreError::InvalidPageLimit);
        }
        let transaction = self.env.read_txn()?;
        let mut output = Vec::with_capacity(limit);
        for result in self.deposits.iter(&transaction)? {
            let (key, value) = result?;
            let id = DepositId::new(
                key.try_into()
                    .map_err(|_| EvmObserverStoreError::CorruptRecord)?,
            );
            if after.is_some_and(|cursor| id <= cursor) {
                continue;
            }
            output.push(decode_deposit(id, value)?);
            if output.len() == limit {
                break;
            }
        }
        Ok(output)
    }

    fn validate_state(&self) -> Result<(), EvmObserverStoreError> {
        let transaction = self.env.read_txn()?;
        let stored_scope = self
            .metadata
            .get(&transaction, SCOPE_KEY)?
            .ok_or(EvmObserverStoreError::CorruptRecord)?;
        if decode_scope(stored_scope)? != self.scope {
            return Err(EvmObserverStoreError::WrongScope);
        }
        let state = self.read_state(&transaction)?;
        if state.checkpoint.next_block < self.scope.first_block {
            return Err(EvmObserverStoreError::CorruptRecord);
        }
        if state.checkpoint == self.scope.initial_checkpoint() {
            if state.last_block.is_some() || self.deposits.len(&transaction)? != 0 {
                return Err(EvmObserverStoreError::CorruptRecord);
            }
        } else {
            let last = state
                .last_block
                .ok_or(EvmObserverStoreError::CorruptRecord)?;
            if last.number.checked_add(1) != Some(state.checkpoint.next_block)
                || last.hash != state.checkpoint.parent_hash
            {
                return Err(EvmObserverStoreError::CorruptRecord);
            }
        }
        for result in self.deposits.iter(&transaction)? {
            let (key, value) = result?;
            let id = DepositId::new(
                key.try_into()
                    .map_err(|_| EvmObserverStoreError::CorruptRecord)?,
            );
            let _stored = decode_deposit(id, value)?;
        }
        Ok(())
    }

    fn read_state(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<StoreState, EvmObserverStoreError> {
        let checkpoint = self
            .metadata
            .get(transaction, CHECKPOINT_KEY)?
            .ok_or(EvmObserverStoreError::CorruptRecord)
            .and_then(decode_checkpoint)?;
        let last_block = self
            .metadata
            .get(transaction, LAST_BLOCK_KEY)?
            .map(decode_last_block)
            .transpose()?;
        Ok(StoreState {
            checkpoint,
            last_block,
        })
    }

    fn require_new_deposits(
        &self,
        transaction: &heed::RoTxn<'_>,
        prepared: &PreparedEvmObserverBlock,
    ) -> Result<(), EvmObserverStoreError> {
        for deposit in prepared.deposits() {
            let id = DepositId::derive(&deposit.claim());
            if self
                .deposits
                .get(transaction, id.as_bytes().as_slice())?
                .is_some()
            {
                return Err(EvmObserverStoreError::ConflictingDeposit);
            }
        }
        Ok(())
    }

    fn require_same_deposits(
        &self,
        transaction: &heed::RoTxn<'_>,
        prepared: &PreparedEvmObserverBlock,
    ) -> Result<(), EvmObserverStoreError> {
        for deposit in prepared.deposits() {
            let id = DepositId::derive(&deposit.claim());
            let expected = encode_deposit(deposit);
            if self.deposits.get(transaction, id.as_bytes().as_slice())?
                != Some(expected.as_slice())
            {
                return Err(EvmObserverStoreError::ConflictingDeposit);
            }
        }
        Ok(())
    }
}

#[allow(unsafe_code)]
fn open_environment(
    path: &Path,
    options: EvmObserverStoreOptions,
) -> Result<Env, EvmObserverStoreError> {
    let mut builder = EnvOpenOptions::new();
    builder
        .map_size(options.map_size)
        .max_dbs(MAX_DATABASES)
        .max_readers(options.max_readers);
    // SAFETY: the canonical directory is service-owned and never modified
    // outside LMDB while this environment is live. Locking and full durability
    // remain enabled, and mapped references never escape transaction lifetimes.
    unsafe { builder.open(path) }.map_err(Into::into)
}
