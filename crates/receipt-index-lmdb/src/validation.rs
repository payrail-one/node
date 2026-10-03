use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexCursor};

use crate::{
    IndexedReceipt, ReceiptIndexStoreError,
    codec::{BlockRecord, decode_block, decode_cursor, receipts_digest},
    read::{account_nonce_key, correlation_key},
    store::{CURSOR_KEY, LmdbReceiptIndex},
};

impl LmdbReceiptIndex {
    pub(crate) fn validate_history(&self) -> Result<(), ReceiptIndexStoreError> {
        let transaction = self.env.read_txn()?;
        let Some(encoded_cursor) = self.metadata.get(&transaction, CURSOR_KEY)? else {
            return self.require_empty(&transaction);
        };
        let stored = decode_cursor(encoded_cursor)?;
        if stored.base.network != self.network {
            return Err(ReceiptIndexStoreError::WrongNetwork);
        }
        let mut cursor = ReceiptIndexCursor::new(stored.base);
        let mut expected_receipts = 0_u64;
        while cursor.latest().height < stored.latest.height {
            let height = cursor
                .latest()
                .height
                .checked_add(1)
                .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
            let block_key = height.to_be_bytes();
            let encoded = self
                .blocks
                .get(&transaction, block_key.as_slice())?
                .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
            let block_record = decode_block(encoded)?;
            if block_record.first_operation_index != cursor.next_operation_index() {
                return Err(ReceiptIndexStoreError::CorruptRecord);
            }
            let indexed = self.read_block_receipts(&transaction, block_record)?;
            let block = FinalizedReceiptBlock {
                network: self.network,
                previous: block_record.previous,
                checkpoint: block_record.checkpoint,
                receipts: indexed.iter().map(|value| value.receipt).collect(),
            };
            let prepared = cursor.prepare(&block)?;
            cursor.commit(prepared)?;
            expected_receipts = expected_receipts
                .checked_add(u64::from(block_record.receipt_count))
                .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
        }
        self.require_final_counts(&transaction, stored, cursor, expected_receipts)
    }

    fn require_empty(&self, transaction: &heed::RoTxn<'_>) -> Result<(), ReceiptIndexStoreError> {
        if self.blocks.len(transaction)? == 0
            && self.receipts.len(transaction)? == 0
            && self.operation_indices.len(transaction)? == 0
            && self.account_nonces.len(transaction)? == 0
            && self.correlations.len(transaction)? == 0
        {
            Ok(())
        } else {
            Err(ReceiptIndexStoreError::CorruptRecord)
        }
    }

    fn require_final_counts(
        &self,
        transaction: &heed::RoTxn<'_>,
        stored: crate::codec::CursorRecord,
        cursor: ReceiptIndexCursor,
        expected_receipts: u64,
    ) -> Result<(), ReceiptIndexStoreError> {
        let expected_blocks = stored
            .latest
            .height
            .checked_sub(stored.base.checkpoint.height)
            .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
        if cursor.latest() != stored.latest
            || cursor.next_operation_index() != stored.next_operation_index
            || self.blocks.len(transaction)? != expected_blocks
            || self.receipts.len(transaction)? != expected_receipts
            || self.operation_indices.len(transaction)? != expected_receipts
            || self.account_nonces.len(transaction)? != expected_receipts
            || self.correlations.len(transaction)? != expected_receipts
        {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(())
    }

    fn read_block_receipts(
        &self,
        transaction: &heed::RoTxn<'_>,
        block: BlockRecord,
    ) -> Result<Vec<IndexedReceipt>, ReceiptIndexStoreError> {
        let mut indexed = Vec::with_capacity(
            usize::try_from(block.receipt_count)
                .map_err(|_| ReceiptIndexStoreError::CorruptRecord)?,
        );
        for offset in 0..block.receipt_count {
            let operation_index = block
                .first_operation_index
                .checked_add(u64::from(offset))
                .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
            let receipt = self
                .receipt_from_database(
                    transaction,
                    self.operation_indices,
                    &operation_index.to_be_bytes(),
                )?
                .ok_or(ReceiptIndexStoreError::CorruptRecord)?;
            if receipt.checkpoint != block.checkpoint || receipt.operation_offset != offset {
                return Err(ReceiptIndexStoreError::CorruptRecord);
            }
            self.require_secondary_indexes(transaction, receipt)?;
            indexed.push(receipt);
        }
        if receipts_digest(&indexed) != block.receipts_digest {
            return Err(ReceiptIndexStoreError::CorruptRecord);
        }
        Ok(indexed)
    }

    pub(crate) fn require_secondary_indexes(
        &self,
        transaction: &heed::RoTxn<'_>,
        value: IndexedReceipt,
    ) -> Result<(), ReceiptIndexStoreError> {
        let receipt = value.receipt;
        for (database, key) in [
            (
                self.operation_indices,
                receipt.operation_index.to_be_bytes().to_vec(),
            ),
            (
                self.account_nonces,
                account_nonce_key(receipt.account, receipt.nonce).to_vec(),
            ),
            (
                self.correlations,
                correlation_key(receipt.account, receipt.idempotency_key, receipt.nonce).to_vec(),
            ),
        ] {
            if database.get(transaction, &key)? != Some(receipt.operation_id.as_bytes().as_slice())
            {
                return Err(ReceiptIndexStoreError::CorruptRecord);
            }
        }
        Ok(())
    }
}
