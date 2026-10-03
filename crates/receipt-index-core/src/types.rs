use ledger_core::{LedgerSnapshot, NetworkId, OperationReceipt};
use state_sync_core::FinalizedCheckpoint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptIndexBase {
    pub network: NetworkId,
    pub checkpoint: FinalizedCheckpoint,
    pub next_operation_index: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedReceiptBlock {
    pub network: NetworkId,
    pub previous: FinalizedCheckpoint,
    pub checkpoint: FinalizedCheckpoint,
    pub receipts: Vec<OperationReceipt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexedFinalizedReceipt {
    pub checkpoint: FinalizedCheckpoint,
    pub receipt: OperationReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedLedgerBase {
    pub checkpoint: FinalizedCheckpoint,
    pub snapshot: LedgerSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchivedLedgerBlock {
    pub previous: FinalizedCheckpoint,
    pub checkpoint: FinalizedCheckpoint,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptIndexWriteOutcome {
    Committed,
    ExistingSame,
}
