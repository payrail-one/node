use ledger_core::NetworkId;
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot};

pub const MAX_TAIL_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedTailBlock {
    pub network: NetworkId,
    pub parent_hash: BlockHash,
    pub checkpoint: FinalizedCheckpoint,
    pub payload: Vec<u8>,
    pub finality_proof: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TailCommitment {
    pub block_hash: BlockHash,
    pub state_root: StateRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedTailTransition {
    pub commitment: TailCommitment,
    pub state: Vec<u8>,
}

pub trait TailTransitionExecutor {
    /// Prepares the next canonical state and commitments without mutating the
    /// authoritative store. Implementations must be deterministic and side-effect free.
    fn prepare_transition(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
    ) -> Option<PreparedTailTransition>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedTailBlock {
    pub(crate) network: NetworkId,
    pub(crate) previous: FinalizedCheckpoint,
    pub(crate) checkpoint: FinalizedCheckpoint,
    pub(crate) payload: Vec<u8>,
    pub(crate) state: Vec<u8>,
}

impl VerifiedTailBlock {
    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn previous(&self) -> FinalizedCheckpoint {
        self.previous
    }

    #[must_use]
    pub const fn checkpoint(&self) -> FinalizedCheckpoint {
        self.checkpoint
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    #[must_use]
    pub fn state(&self) -> &[u8] {
        &self.state
    }
}
