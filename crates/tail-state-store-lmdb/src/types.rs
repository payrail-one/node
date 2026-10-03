use ledger_core::LedgerSnapshot;
use ledger_runtime_core::LedgerStateProof;
use state_sync_core::{FinalizedCheckpoint, StateRoot};

pub const MINIMUM_MAP_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LmdbStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for LmdbStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 256 * 1024 * 1024,
            max_readers: 126,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTailState {
    pub checkpoint: FinalizedCheckpoint,
    pub state: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredFinalizedBlock {
    pub previous: FinalizedCheckpoint,
    pub checkpoint: FinalizedCheckpoint,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredLedgerState {
    pub checkpoint: FinalizedCheckpoint,
    pub snapshot: LedgerSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredAuthenticatedState {
    pub base_height: u64,
    pub latest_height: u64,
    pub tree_version: u64,
    pub root: StateRoot,
}

#[derive(Clone, Debug)]
pub struct StoredLedgerProof {
    pub height: u64,
    pub root: StateRoot,
    pub proof: LedgerStateProof,
}

impl StoredLedgerProof {
    #[must_use]
    pub fn verifies(&self) -> bool {
        self.proof.verifies(self.root)
    }

    #[must_use]
    pub fn value(&self) -> Option<&[u8]> {
        self.proof.value()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    Committed,
    ExistingSame,
}
