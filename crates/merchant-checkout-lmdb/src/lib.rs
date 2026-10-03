#![deny(unsafe_code)]

mod codec;
mod environment;
mod error;
mod port;
mod store;
mod validation;

pub use error::MerchantCheckoutStoreError;
pub use store::LmdbMerchantCheckoutStore;

pub const MINIMUM_MAP_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MerchantCheckoutStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for MerchantCheckoutStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_readers: 126,
        }
    }
}
