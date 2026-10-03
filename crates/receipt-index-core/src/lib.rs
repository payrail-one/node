#![forbid(unsafe_code)]

mod cursor;
mod error;
mod port;
mod types;

pub use cursor::{PreparedReceiptIndexAdvance, ReceiptIndexCursor};
pub use error::ReceiptIndexError;
pub use port::{FinalizedLedgerHistory, ReceiptIndexStore};
pub use types::{
    ArchivedLedgerBlock, FinalizedLedgerBase, FinalizedReceiptBlock, IndexedFinalizedReceipt,
    ReceiptIndexBase, ReceiptIndexWriteOutcome,
};
