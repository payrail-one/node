#![forbid(unsafe_code)]

mod codec;
mod error;
mod finality;
mod identity;
mod ingestion;
mod observation;
mod observer;
mod quorum;
mod types;

pub use codec::decode_deposit;
pub use error::{EvmDepositError, EvmDepositVerificationError};
pub use finality::{EvmFinalitySource, verify_finalized_deposit};
pub use identity::{origin_asset_id, origin_network_id};
pub use ingestion::{EvmObserverIngestionError, ingest_next_observer_block};
pub use observation::{
    EvmBlockObservationProvider, EvmBlockObservationQuorum, EvmBlockObservationQuorumError,
    EvmBlockObservationSource,
};
pub use observer::{
    EvmObserverCheckpoint, EvmObserverError, EvmObserverScope, FinalizedEvmObserverBlock,
    MAX_DEPOSITS_PER_BLOCK, PreparedEvmObserverBlock, prepare_observer_block,
};
pub use quorum::{
    EvmFinalityProvider, EvmFinalityQuorum, EvmFinalityQuorumError, MAX_FINALITY_PROVIDERS,
};
pub use types::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmChainId, EvmFinalizedBlock,
    EvmHash, EvmLog, EvmObservedBlock, EvmProviderFinality, EvmProviderId, ObservedEvmDeposit,
    VerifiedEvmDeposit,
};
