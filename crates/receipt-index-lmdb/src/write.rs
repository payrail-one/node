use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexCursor};

use crate::{
    IndexedReceipt, ReceiptIndexCommitOutcome, ReceiptIndexStoreError,
    codec::{
        BlockRecord, CursorRecord, decode_block, encode_block, encode_cursor, encode_receipt,
        receipts_digest,
    },
    read::{account_nonce_key, correlation_key},
    store::{CURSOR_KEY, LmdbReceiptIndex},
};

impl LmdbReceiptIndex {
    /// Atomically indexes one sequential finalized block and all its receipts.
    ///
    /// # Errors
    ///
    /// Returns an error for a gap/fork, malformed sequence, duplicate identity,
    /// conflicting retry or LMDB failure. No partial index is published.
    pub fn commit_finalized(
        &self,
        block: &FinalizedReceiptBlock,
    ) -> Result<ReceiptIndexCommitOutcome, ReceiptIndexStoreError> {
        if block.network != self.network {
            return Err(ReceiptIndexStoreError::WrongNetwork);
        }
        let mut transaction = self.env.write_txn()?;
        let current = self.read_cursor(&transaction)?;
        let indexed = indexed_receipts(block)?;
        let first_operation_index = indexed
            .first()
            .map_or(current.next_operation_index, |value| {
                value.receipt.operation_index
            });
        let requested = block_record(first_operation_index, block, &indexed)?;
        let block_key = block.checkpoint.height.to_be_bytes();

        if current.latest == block.checkpoint {
            self.require_existing_same(&transaction, block_key, requested, &indexed)?;
            return Ok(ReceiptIndexCommitOutcome::ExistingSame);
        }
        let cursor = ReceiptIndexCursor::restore(
            current.base,
            current.latest,
            current.next_operation_index,
        )?;
        let prepared = cursor.prepare(block)?;
        if self
            .blocks
            .get(&transaction, block_key.as_slice())?
            .is_some()
        {
            return Err(ReceiptIndexStoreError::ConflictingBlock);
        }
        self.require_new_keys(&transaction, &indexed)?;
        for receipt in &indexed {
            self.write_receipt(&mut transaction, *receipt)?;
        }
        let encoded_block = encode_block(requested);
        self.blocks.put(
            &mut transaction,
            block_key.as_slice(),
            encoded_block.as_slice(),
        )?;
        let next = prepared.next_cursor();
        let encoded_cursor = encode_cursor(CursorRecord {
            base: current.base,
            latest: next.latest(),
            next_operation_index: next.next_operation_index(),
        });
        self.metadata
            .put(&mut transaction, CURSOR_KEY, encoded_cursor.as_slice())?;
        transaction.commit()?;
        Ok(ReceiptIndexCommitOutcome::Committed)
    }

    fn require_new_keys(
        &self,
        transaction: &heed::RoTxn<'_>,
        indexed: &[IndexedReceipt],
    ) -> Result<(), ReceiptIndexStoreError> {
        for value in indexed {
            let receipt = value.receipt;
            if self
                .receipts
                .get(transaction, receipt.operation_id.as_bytes().as_slice())?
                .is_some()
            {
                return Err(ReceiptIndexStoreError::DuplicateOperation);
            }
            if self
                .operation_indices
                .get(transaction, &receipt.operation_index.to_be_bytes())?
                .is_some()
            {
                return Err(ReceiptIndexStoreError::ConflictingBlock);
            }
            if self
                .account_nonces
                .get(
                    transaction,
                    &account_nonce_key(receipt.account, receipt.nonce),
                )?
                .is_some()
            {
                return Err(ReceiptIndexStoreError::DuplicateAccountNonce);
            }
            if self
                .correlations
                .get(
                    transaction,
                    &correlation_key(receipt.account, receipt.idempotency_key, receipt.nonce),
                )?
                .is_some()
            {
                return Err(ReceiptIndexStoreError::ConflictingBlock);
            }
        }
        Ok(())
    }

    fn write_receipt(
        &self,
        transaction: &mut heed::RwTxn<'_>,
        value: IndexedReceipt,
    ) -> Result<(), ReceiptIndexStoreError> {
        let receipt = value.receipt;
        let encoded = encode_receipt(value);
        self.receipts.put(
            transaction,
            receipt.operation_id.as_bytes().as_slice(),
            encoded.as_slice(),
        )?;
        self.operation_indices.put(
            transaction,
            &receipt.operation_index.to_be_bytes(),
            receipt.operation_id.as_bytes().as_slice(),
        )?;
        self.account_nonces.put(
            transaction,
            &account_nonce_key(receipt.account, receipt.nonce),
            receipt.operation_id.as_bytes().as_slice(),
        )?;
        self.correlations.put(
            transaction,
            &correlation_key(receipt.account, receipt.idempotency_key, receipt.nonce),
            receipt.operation_id.as_bytes().as_slice(),
        )?;
        Ok(())
    }

    fn require_existing_same(
        &self,
        transaction: &heed::RoTxn<'_>,
        block_key: [u8; 8],
        requested: BlockRecord,
        indexed: &[IndexedReceipt],
    ) -> Result<(), ReceiptIndexStoreError> {
        let existing = self
            .blocks
            .get(transaction, block_key.as_slice())?
            .ok_or(ReceiptIndexStoreError::ConflictingBlock)?;
        if decode_block(existing)? != requested
            || receipts_digest(indexed) != requested.receipts_digest
        {
            return Err(ReceiptIndexStoreError::ConflictingBlock);
        }
        for value in indexed {
            let stored = self
                .receipt_for_id(transaction, value.receipt.operation_id)?
                .ok_or(ReceiptIndexStoreError::ConflictingBlock)?;
            if stored != *value {
                return Err(ReceiptIndexStoreError::ConflictingBlock);
            }
            self.require_secondary_indexes(transaction, stored)?;
        }
        Ok(())
    }
}

fn indexed_receipts(
    block: &FinalizedReceiptBlock,
) -> Result<Vec<IndexedReceipt>, ReceiptIndexStoreError> {
    block
        .receipts
        .iter()
        .enumerate()
        .map(|(offset, receipt)| {
            Ok(IndexedReceipt {
                checkpoint: block.checkpoint,
                operation_offset: u32::try_from(offset)
                    .map_err(|_| ReceiptIndexStoreError::CorruptRecord)?,
                receipt: *receipt,
            })
        })
        .collect()
}

fn block_record(
    first_operation_index: u64,
    block: &FinalizedReceiptBlock,
    receipts: &[IndexedReceipt],
) -> Result<BlockRecord, ReceiptIndexStoreError> {
    Ok(BlockRecord {
        previous: block.previous,
        checkpoint: block.checkpoint,
        first_operation_index,
        receipt_count: u32::try_from(receipts.len())
            .map_err(|_| ReceiptIndexStoreError::CorruptRecord)?,
        receipts_digest: receipts_digest(receipts),
    })
}
