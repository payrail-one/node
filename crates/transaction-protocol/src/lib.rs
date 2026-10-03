#![forbid(unsafe_code)]

mod decoder;
mod envelope;
mod error;

pub use envelope::{MAX_ENVELOPE_BYTES, SignedOperationCodec};
pub use error::ProtocolError;
