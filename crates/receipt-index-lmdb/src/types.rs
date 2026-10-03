use ledger_core::OperationReceipt;
use state_sync_core::FinalizedCheckpoint;

pub const MINIMUM_MAP_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptIndexStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for ReceiptIndexStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_readers: 126,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexedReceipt {
    pub checkpoint: FinalizedCheckpoint,
    pub operation_offset: u32,
    pub receipt: OperationReceipt,
}
