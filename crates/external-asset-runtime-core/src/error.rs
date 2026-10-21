use external_asset_settlement_core::ExternalSettlementError;
use ledger_core::LedgerError;
use ledger_runtime_core::RuntimeError;

use crate::ExternalSettlementStateCodecError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalSettlementRuntimeError {
    BlockTooLarge,
    InvalidDomain,
    InvalidOperation,
    UnexpectedEnd,
    TrailingBytes,
    WrongNetwork,
    WrongAsset,
    WrongAuthority,
    InvalidAuthoritySignature,
    PreviousStateRootMismatch,
    InvalidStateHeight,
    LegacyState(LedgerError),
    LedgerRuntime(RuntimeError),
    State(ExternalSettlementStateCodecError),
    Ledger(LedgerError),
    Settlement(ExternalSettlementError),
}

impl From<ExternalSettlementStateCodecError> for ExternalSettlementRuntimeError {
    fn from(error: ExternalSettlementStateCodecError) -> Self {
        Self::State(error)
    }
}

impl From<ExternalSettlementError> for ExternalSettlementRuntimeError {
    fn from(error: ExternalSettlementError) -> Self {
        Self::Settlement(error)
    }
}

impl From<RuntimeError> for ExternalSettlementRuntimeError {
    fn from(error: RuntimeError) -> Self {
        Self::LedgerRuntime(error)
    }
}
