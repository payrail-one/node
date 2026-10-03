#![deny(unsafe_code)]

mod codec;
mod error;
mod read;
mod store;
mod types;
mod validation;
mod write;

pub use error::ReceiptIndexStoreError;
pub use receipt_index_core::ReceiptIndexWriteOutcome as ReceiptIndexCommitOutcome;
pub use store::LmdbReceiptIndex;
pub use types::{IndexedReceipt, MINIMUM_MAP_SIZE, ReceiptIndexStoreOptions};
