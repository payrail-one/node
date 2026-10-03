use ledger_core::{NetworkId, OperationId, OperationReceipt};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    ArchivedLedgerBlock, FinalizedLedgerBase, FinalizedReceiptBlock, IndexedFinalizedReceipt,
    ReceiptIndexBase, ReceiptIndexCursor, ReceiptIndexWriteOutcome,
};

pub trait FinalizedLedgerHistory {
    type Error;

    /// Reads the invariant-checked ledger snapshot anchoring retained history.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the base is missing, corrupt or unreadable.
    fn recovery_base(&self) -> Result<FinalizedLedgerBase, Self::Error>;

    /// Reads the latest finalized source checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when finalized state is unavailable or corrupt.
    fn latest_checkpoint(&self) -> Result<FinalizedCheckpoint, Self::Error>;

    /// Reads one complete retained finalized block.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when the height is absent, corrupt or unreadable.
    fn finalized_block(&self, height: u64) -> Result<ArchivedLedgerBlock, Self::Error>;
}

pub trait ReceiptIndexStore {
    type Error;

    fn network(&self) -> NetworkId;

    /// Reads the validated index cursor, or `None` before initialization.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when index metadata is corrupt or unreadable.
    fn cursor(&self) -> Result<Option<ReceiptIndexCursor>, Self::Error>;

    /// Anchors an empty derived index at a verified ledger base.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for prior initialization, mismatch or I/O failure.
    fn initialize(&self, base: ReceiptIndexBase) -> Result<(), Self::Error>;

    /// Atomically publishes one sequential finalized receipt batch.
    ///
    /// # Errors
    ///
    /// Returns the adapter error for a conflict, invalid cursor or write failure.
    fn commit_finalized(
        &self,
        block: &FinalizedReceiptBlock,
    ) -> Result<ReceiptIndexWriteOutcome, Self::Error>;

    /// Reads one finalized receipt by its canonical operation identifier.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when an index record is corrupt or unreadable.
    fn receipt_by_operation_id(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<OperationReceipt>, Self::Error> {
        self.finalized_receipt_by_operation_id(operation_id)
            .map(|value| value.map(|indexed| indexed.receipt))
    }

    /// Reads a finalized receipt together with its verified block checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when an index record is corrupt or unreadable.
    fn finalized_receipt_by_operation_id(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<IndexedFinalizedReceipt>, Self::Error>;
}
