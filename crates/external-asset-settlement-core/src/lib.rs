#![forbid(unsafe_code)]

mod authorization;
mod error;
mod settlement;

pub use authorization::{AuthorizedWithdrawal, SignedWithdrawalRequest};
pub use error::ExternalSettlementError;
pub use settlement::{ExternalAssetSettlement, ExternalAssetSettlementSnapshot, SettlementOutcome};
