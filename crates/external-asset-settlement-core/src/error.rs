use external_asset_core::ExternalAssetError;
use ledger_core::LedgerError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalSettlementError {
    External(ExternalAssetError),
    Ledger(LedgerError),
    AssetDefinitionMismatch,
    SupplyMismatch,
    BackingMismatch,
    InstructionMismatch,
    WrongWithdrawalNetwork,
    ExpiredWithdrawalAuthorization,
    InvalidWithdrawalSignature,
}

impl From<ExternalAssetError> for ExternalSettlementError {
    fn from(error: ExternalAssetError) -> Self {
        Self::External(error)
    }
}

impl From<LedgerError> for ExternalSettlementError {
    fn from(error: LedgerError) -> Self {
        Self::Ledger(error)
    }
}
