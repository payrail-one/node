#![forbid(unsafe_code)]

mod account;
mod asset;
mod authorization;
mod authorization_identity;
mod contract;
mod error;
mod event;
mod genesis;
mod ledger;
mod transaction;
mod types;

pub use account::AccountStatus;
pub use asset::{AssetAudit, AssetClass, AssetDefinition, AssetStatus, BackingRequirement};
pub use authorization::{
    Authorization, AuthorizationRole, AuthorizedOperation, SignatureBytes, SignatureVerification,
    SignatureVerifier, SignedOperation, VerifiedOperation, verify_operation_batch,
};
pub use authorization_identity::{VerifiedAuthorizationId, VerifiedAuthorizationIdentity};
pub use contract::{
    ContractCall, ContractDeploy, ContractEventRecord, ContractRecord, ContractStateEntry,
    MAX_CONTRACT_ARGS_BYTES, MAX_CONTRACT_CODE_BYTES, MAX_CONTRACT_EXECUTION_UNITS, contract_id,
};
pub use error::LedgerError;
pub use event::Event;
pub use genesis::{GenesisAsset, GenesisBalance, GenesisConfig};
pub use ledger::Ledger;
pub use ledger::snapshot::{
    LedgerSnapshot, SnapshotAccountStatus, SnapshotAsset, SnapshotBalance, SnapshotNonce,
};
pub use transaction::{
    MAX_BATCH_ITEMS, OperationKind, OperationOutcome, OperationReceipt, Transfer, TransferBatch,
    TransferItem,
};
pub use types::{
    AccountId, AssetId, Balance, ContractId, IdempotencyKey, NetworkId, Nonce, OperationId,
};

pub(crate) use asset::AssetState;
