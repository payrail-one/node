use std::collections::BTreeSet;

use ledger_core::{AccountId, NetworkId, Nonce, OperationId};
use ledger_runtime_core::MAX_BLOCK_OPERATIONS;
use state_sync_core::FinalizedCheckpoint;

use crate::{FinalizedReceiptBlock, ReceiptIndexBase, ReceiptIndexError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptIndexCursor {
    network: NetworkId,
    base: FinalizedCheckpoint,
    base_operation_index: u64,
    latest: FinalizedCheckpoint,
    next_operation_index: u64,
}

impl ReceiptIndexCursor {
    #[must_use]
    pub const fn new(base: ReceiptIndexBase) -> Self {
        Self {
            network: base.network,
            base: base.checkpoint,
            base_operation_index: base.next_operation_index,
            latest: base.checkpoint,
            next_operation_index: base.next_operation_index,
        }
    }

    /// Restores a persisted cursor after structural validation.
    ///
    /// Full block/receipt history must be validated by the storage adapter
    /// before it exposes this cursor as trusted state.
    ///
    /// # Errors
    ///
    /// Returns an error for a height rollback, a sequence rollback or an
    /// inconsistent cursor at the base height.
    pub fn restore(
        base: ReceiptIndexBase,
        latest: FinalizedCheckpoint,
        next_operation_index: u64,
    ) -> Result<Self, ReceiptIndexError> {
        if latest.height < base.checkpoint.height
            || next_operation_index < base.next_operation_index
            || (latest.height == base.checkpoint.height
                && (latest != base.checkpoint || next_operation_index != base.next_operation_index))
        {
            return Err(ReceiptIndexError::CursorMismatch);
        }
        Ok(Self {
            network: base.network,
            base: base.checkpoint,
            base_operation_index: base.next_operation_index,
            latest,
            next_operation_index,
        })
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn base(&self) -> FinalizedCheckpoint {
        self.base
    }

    #[must_use]
    pub const fn base_operation_index(&self) -> u64 {
        self.base_operation_index
    }

    #[must_use]
    pub const fn latest(&self) -> FinalizedCheckpoint {
        self.latest
    }

    #[must_use]
    pub const fn next_operation_index(&self) -> u64 {
        self.next_operation_index
    }

    /// Validates one sequential finalized receipt batch without changing the cursor.
    ///
    /// # Errors
    ///
    /// Returns an error for another network, a gap/fork, a non-contiguous
    /// operation sequence, duplicate identifiers or arithmetic overflow.
    pub fn prepare(
        &self,
        block: &FinalizedReceiptBlock,
    ) -> Result<PreparedReceiptIndexAdvance, ReceiptIndexError> {
        validate_header(*self, block)?;
        validate_receipts(self.next_operation_index, &block.receipts)?;
        let receipt_count =
            u64::try_from(block.receipts.len()).map_err(|_| ReceiptIndexError::TooManyReceipts)?;
        let next_operation_index = self
            .next_operation_index
            .checked_add(receipt_count)
            .ok_or(ReceiptIndexError::ArithmeticOverflow)?;
        Ok(PreparedReceiptIndexAdvance {
            previous: *self,
            next: Self {
                network: self.network,
                base: self.base,
                base_operation_index: self.base_operation_index,
                latest: block.checkpoint,
                next_operation_index,
            },
        })
    }

    /// Publishes an already validated advance only if its predecessor is current.
    ///
    /// # Errors
    ///
    /// Returns an error when another advance has already changed the cursor.
    pub fn commit(
        &mut self,
        prepared: PreparedReceiptIndexAdvance,
    ) -> Result<(), ReceiptIndexError> {
        if *self != prepared.previous {
            return Err(ReceiptIndexError::StalePreparedAdvance);
        }
        *self = prepared.next;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedReceiptIndexAdvance {
    previous: ReceiptIndexCursor,
    next: ReceiptIndexCursor,
}

impl PreparedReceiptIndexAdvance {
    #[must_use]
    pub const fn next_cursor(&self) -> ReceiptIndexCursor {
        self.next
    }
}

fn validate_header(
    cursor: ReceiptIndexCursor,
    block: &FinalizedReceiptBlock,
) -> Result<(), ReceiptIndexError> {
    if block.network != cursor.network {
        return Err(ReceiptIndexError::WrongNetwork);
    }
    let expected_height = cursor
        .latest
        .height
        .checked_add(1)
        .ok_or(ReceiptIndexError::ArithmeticOverflow)?;
    if block.previous != cursor.latest
        || block.checkpoint.height != expected_height
        || block.checkpoint.block_hash == cursor.latest.block_hash
    {
        return Err(ReceiptIndexError::CursorMismatch);
    }
    Ok(())
}

fn validate_receipts(
    first_operation_index: u64,
    receipts: &[ledger_core::OperationReceipt],
) -> Result<(), ReceiptIndexError> {
    if receipts.len() > MAX_BLOCK_OPERATIONS {
        return Err(ReceiptIndexError::TooManyReceipts);
    }
    let mut operation_ids = BTreeSet::<OperationId>::new();
    let mut account_nonces = BTreeSet::<(AccountId, Nonce)>::new();
    for (offset, receipt) in receipts.iter().enumerate() {
        let offset = u64::try_from(offset).map_err(|_| ReceiptIndexError::TooManyReceipts)?;
        let expected = first_operation_index
            .checked_add(offset)
            .ok_or(ReceiptIndexError::ArithmeticOverflow)?;
        if receipt.operation_index != expected {
            return Err(ReceiptIndexError::ReceiptSequenceMismatch {
                expected,
                actual: receipt.operation_index,
            });
        }
        if !operation_ids.insert(receipt.operation_id) {
            return Err(ReceiptIndexError::DuplicateOperation);
        }
        if !account_nonces.insert((receipt.account, receipt.nonce)) {
            return Err(ReceiptIndexError::DuplicateAccountNonce);
        }
    }
    Ok(())
}
