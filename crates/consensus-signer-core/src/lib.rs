#![forbid(unsafe_code)]

mod error;
mod guard;
mod types;

pub use error::ConsensusSignerError;
pub use guard::ConsensusSigningGuard;
pub use types::{
    ConsensusSignature, GuardedSignature, Reservation, SigningJournal, VoteIntent,
    VotePayloadEncoder, VoteSigner, VoteSlot, VoteStage,
};
