#![forbid(unsafe_code)]

mod codec_error;
mod coordinator;
mod error;
mod state_codec;
mod state_decoder;
mod types;

pub use codec_error::ExternalAssetStateCodecError;
pub use coordinator::ExternalAssetCoordinator;
pub use coordinator::snapshot::ExternalAssetSnapshot;
pub use error::ExternalAssetError;
pub use state_codec::{ExternalAssetStateCodec, MAX_EXTERNAL_ASSET_STATE_BYTES};
pub use types::{
    AssetSettlementAudit, ConnectorStatus, DepositId, DepositRecord, DepositStatus,
    DestinationCommitment, ExternalAssetConfig, ExternalAssetEvent, FinalityEvidence,
    MintInstruction, OriginAssetId, OriginDeposit, OriginNetworkId, OriginReleaseReference,
    OriginTransactionId, ReleaseInstruction, SettlementReference, WithdrawalId, WithdrawalRecord,
    WithdrawalRequest, WithdrawalStatus,
};
