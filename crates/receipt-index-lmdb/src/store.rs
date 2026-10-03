use std::{fs, path::Path};

use heed::{Database, Env, EnvOpenOptions, types::Bytes};
use ledger_core::NetworkId;
use receipt_index_core::{ReceiptIndexBase, ReceiptIndexCursor};

use crate::{
    ReceiptIndexStoreError, ReceiptIndexStoreOptions,
    codec::{CursorRecord, decode_cursor, encode_cursor},
    types::MINIMUM_MAP_SIZE,
};

const NETWORK_KEY: &[u8] = b"network";
pub(crate) const CURSOR_KEY: &[u8] = b"cursor";
const MAX_DATABASES: u32 = 10;

#[derive(Debug)]
pub struct LmdbReceiptIndex {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) blocks: Database<Bytes, Bytes>,
    pub(crate) receipts: Database<Bytes, Bytes>,
    pub(crate) operation_indices: Database<Bytes, Bytes>,
    pub(crate) account_nonces: Database<Bytes, Bytes>,
    pub(crate) correlations: Database<Bytes, Bytes>,
    pub(crate) network: NetworkId,
}

impl LmdbReceiptIndex {
    /// Opens an independent derived receipt index using safe LMDB durability flags.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe path, invalid options, another network,
    /// corrupt index history or an LMDB failure.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: ReceiptIndexStoreOptions,
    ) -> Result<Self, ReceiptIndexStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(ReceiptIndexStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ReceiptIndexStoreError::UnsafePath);
        }
        let path = fs::canonicalize(requested)?;
        let env = open_environment(&path, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let blocks = env.create_database(&mut transaction, Some("blocks"))?;
        let receipts = env.create_database(&mut transaction, Some("receipts"))?;
        let operation_indices = env.create_database(&mut transaction, Some("operation_indices"))?;
        let account_nonces = env.create_database(&mut transaction, Some("account_nonces"))?;
        let correlations = env.create_database(&mut transaction, Some("correlations"))?;
        match metadata.get(&transaction, NETWORK_KEY)? {
            Some(stored) if stored != network.as_bytes() => {
                return Err(ReceiptIndexStoreError::WrongNetwork);
            }
            Some(_) => {}
            None => metadata.put(&mut transaction, NETWORK_KEY, network.as_bytes().as_slice())?,
        }
        transaction.commit()?;
        let store = Self {
            env,
            metadata,
            blocks,
            receipts,
            operation_indices,
            account_nonces,
            correlations,
            network,
        };
        store.validate_history()?;
        Ok(store)
    }

    /// Initializes the derived index at a verified recovery base.
    ///
    /// The base operation index comes from the validated ledger snapshot. The
    /// index intentionally contains no receipt history before that checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for another network, prior initialization or LMDB failure.
    pub fn initialize(&self, base: ReceiptIndexBase) -> Result<(), ReceiptIndexStoreError> {
        if base.network != self.network {
            return Err(ReceiptIndexStoreError::WrongNetwork);
        }
        let mut transaction = self.env.write_txn()?;
        if self.metadata.get(&transaction, CURSOR_KEY)?.is_some() {
            return Err(ReceiptIndexStoreError::AlreadyInitialized);
        }
        if self.blocks.len(&transaction)? != 0
            || self.receipts.len(&transaction)? != 0
            || self.operation_indices.len(&transaction)? != 0
            || self.account_nonces.len(&transaction)? != 0
            || self.correlations.len(&transaction)? != 0
        {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        let encoded = encode_cursor(CursorRecord {
            base,
            latest: base.checkpoint,
            next_operation_index: base.next_operation_index,
        });
        self.metadata
            .put(&mut transaction, CURSOR_KEY, encoded.as_slice())?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns the validated derived-index cursor.
    ///
    /// # Errors
    ///
    /// Returns an error when the index is uninitialized or the cursor is corrupt.
    pub fn cursor(&self) -> Result<ReceiptIndexCursor, ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        let record = self.read_cursor(&transaction)?;
        ReceiptIndexCursor::restore(record.base, record.latest, record.next_operation_index)
            .map_err(Into::into)
    }

    pub(crate) fn read_cursor(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<CursorRecord, ReceiptIndexStoreError> {
        let encoded = self
            .metadata
            .get(transaction, CURSOR_KEY)?
            .ok_or(ReceiptIndexStoreError::NotInitialized)?;
        let cursor = decode_cursor(encoded)?;
        if cursor.base.network != self.network {
            return Err(ReceiptIndexStoreError::WrongNetwork);
        }
        Ok(cursor)
    }
}

impl receipt_index_core::ReceiptIndexStore for LmdbReceiptIndex {
    type Error = ReceiptIndexStoreError;

    fn network(&self) -> NetworkId {
        self.network
    }

    fn cursor(&self) -> Result<Option<ReceiptIndexCursor>, Self::Error> {
        match LmdbReceiptIndex::cursor(self) {
            Ok(cursor) => Ok(Some(cursor)),
            Err(ReceiptIndexStoreError::NotInitialized) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn initialize(&self, base: ReceiptIndexBase) -> Result<(), Self::Error> {
        LmdbReceiptIndex::initialize(self, base)
    }

    fn commit_finalized(
        &self,
        block: &receipt_index_core::FinalizedReceiptBlock,
    ) -> Result<receipt_index_core::ReceiptIndexWriteOutcome, Self::Error> {
        LmdbReceiptIndex::commit_finalized(self, block)
    }

    fn finalized_receipt_by_operation_id(
        &self,
        operation_id: ledger_core::OperationId,
    ) -> Result<Option<receipt_index_core::IndexedFinalizedReceipt>, Self::Error> {
        LmdbReceiptIndex::by_operation_id(self, operation_id).map(|value| {
            value.map(|indexed| receipt_index_core::IndexedFinalizedReceipt {
                checkpoint: indexed.checkpoint,
                receipt: indexed.receipt,
            })
        })
    }
}

#[allow(unsafe_code)]
fn open_environment(
    path: &Path,
    options: ReceiptIndexStoreOptions,
) -> Result<Env, ReceiptIndexStoreError> {
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
