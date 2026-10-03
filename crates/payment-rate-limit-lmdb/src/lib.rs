#![deny(unsafe_code)]

mod admission;
mod codec;
mod environment;
mod error;
mod store;
mod validation;

pub use error::LmdbRateLimitError;
pub use store::LmdbPaymentRateLimiter;

pub const MINIMUM_MAP_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LmdbRateLimitOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for LmdbRateLimitOptions {
    fn default() -> Self {
        Self {
            map_size: 64 * 1024 * 1024,
            max_readers: 126,
        }
    }
}
