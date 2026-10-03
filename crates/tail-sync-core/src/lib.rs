#![forbid(unsafe_code)]

mod codec;
mod error;
mod session;
mod types;

pub use codec::{MAX_ENCODED_TAIL_BLOCK_BYTES, TailBlockCodec};
pub use error::TailSyncError;
pub use session::TailSyncSession;
pub use types::{
    FinalizedTailBlock, MAX_TAIL_PAYLOAD_BYTES, PreparedTailTransition, TailCommitment,
    TailTransitionExecutor, VerifiedTailBlock,
};
