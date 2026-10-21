#![deny(unsafe_code)]

mod codec;
mod error;
mod store;
mod types;

pub use error::EvmObserverStoreError;
pub use store::LmdbEvmObserverStore;
pub use types::{
    EvmObserverCommitOutcome, EvmObserverStoreOptions, MAX_DEPOSIT_PAGE_SIZE, MINIMUM_MAP_SIZE,
    StoredEvmDeposit,
};
