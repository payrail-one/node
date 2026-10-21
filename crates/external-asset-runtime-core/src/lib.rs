#![forbid(unsafe_code)]

mod block_codec;
mod error;
mod executor;
mod operation;
mod state_codec;

pub use block_codec::ExternalSettlementBlockCodec;
pub use error::ExternalSettlementRuntimeError;
pub use executor::{
    ExecutedExternalSettlementBlock, ExternalSettlementBlockExecutor,
    ExternalSettlementExecutionKind, ExternalSettlementRuntimeConfig,
};
pub use operation::{
    BridgeAuthorizationRole, CompleteWithdrawalOperation, DepositSettlementOperation,
    ExternalSettlementOperation, WithdrawalSettlementOperation,
};
pub use state_codec::{
    ExternalSettlementStateCodec, ExternalSettlementStateCodecError,
    MAX_EXTERNAL_SETTLEMENT_STATE_BYTES,
};
