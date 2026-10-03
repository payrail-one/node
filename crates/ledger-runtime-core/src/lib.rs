#![forbid(unsafe_code)]

mod authenticated_state;
mod block_codec;
mod commitment;
mod compact_block;
mod decoder;
mod error;
mod executor;
mod proposal;
mod rows;
mod state_codec;
mod state_commitment;
mod verification;

pub use authenticated_state::{
    LedgerAuthenticatedState, LedgerStateNamespace, LedgerStateProof,
    PreparedLedgerAuthenticatedUpdate, authenticated_state_delta, authenticated_state_entries,
    authenticated_state_root,
};
pub use block_codec::{LedgerBlockCodec, MAX_BLOCK_OPERATIONS};
pub use commitment::{block_hash, state_root};
pub use compact_block::{BlockPayloadHash, CompactLedgerBlockManifest, block_payload_hash};
pub use error::RuntimeError;
pub use executor::{ExecutedLedgerBlock, LedgerBlockExecutor, PreparedIncrementalLedgerBlock};
pub use proposal::{
    BlockCandidate, BlockProposal, DEFAULT_MAX_EXPIRED_OPERATIONS, MAX_PROPOSAL_CANDIDATES,
    MAX_PROPOSAL_PASSES, PreparedIncrementalBlockProposal, ProposalLimits, ProposalRejection,
    RejectedCandidate,
};
pub use rows::{LedgerRow, LedgerStateRows};
pub use state_codec::{LedgerStateCodec, MAX_LEDGER_STATE_BYTES};
pub use state_commitment::{StateCommitmentPolicy, StateCommitmentScheme};
pub use verification::{
    DEFAULT_PARALLEL_SIGNATURE_THRESHOLD, MAX_SIGNATURE_WORKERS, SignatureVerificationPolicy,
};
