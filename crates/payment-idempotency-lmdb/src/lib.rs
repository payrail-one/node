#![deny(unsafe_code)]

mod codec;
mod coordination;
mod environment;
mod error;
mod port;
mod reconciliation_queue;
mod store;
mod validation;
mod work_codec;

pub use error::PaymentIdempotencyStoreError;
pub use store::LmdbPaymentIdempotencyStore;

pub const MINIMUM_MAP_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentIdempotencyStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for PaymentIdempotencyStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_readers: 126,
        }
    }
}
