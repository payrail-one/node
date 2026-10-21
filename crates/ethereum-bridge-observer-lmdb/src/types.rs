use ethereum_bridge_core::EvmObserverCheckpoint;
use external_asset_core::{DepositId, FinalityEvidence, OriginDeposit};

pub const MINIMUM_MAP_SIZE: usize = 1_048_576;
pub const MAX_DEPOSIT_PAGE_SIZE: usize = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmObserverStoreOptions {
    pub map_size: usize,
    pub max_readers: u32,
}

impl Default for EvmObserverStoreOptions {
    fn default() -> Self {
        Self {
            map_size: 64 * MINIMUM_MAP_SIZE,
            max_readers: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmObserverCommitOutcome {
    Committed,
    ExistingSame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredEvmDeposit {
    pub id: DepositId,
    pub claim: OriginDeposit,
    pub evidence: FinalityEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LastBlockRecord {
    pub number: u64,
    pub hash: ethereum_bridge_core::EvmHash,
    pub digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoreState {
    pub checkpoint: EvmObserverCheckpoint,
    pub last_block: Option<LastBlockRecord>,
}
