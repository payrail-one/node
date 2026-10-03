#![deny(unsafe_code)]

mod codec;
mod environment;
mod error;
mod port;
mod store;
mod validation;

pub use error::ApprovalCodeStoreError;
pub use store::LmdbApprovalCodeStore;

pub const MINIMUM_MAP_SIZE: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalCodeStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for ApprovalCodeStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 64 * 1024 * 1024,
            max_readers: 126,
        }
    }
}
