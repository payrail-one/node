use crate::{AccountId, AssetId, Balance, IdempotencyKey, NetworkId, Nonce, OperationId};

pub const MAX_BATCH_ITEMS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Transfer,
    SponsoredTransfer,
    BatchTransfer,
    SponsoredBatchTransfer,
    ContractDeploy,
    ContractCall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationOutcome {
    Applied,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationReceipt {
    pub operation_id: OperationId,
    pub account: AccountId,
    pub idempotency_key: IdempotencyKey,
    pub nonce: Nonce,
    pub operation_index: u64,
    pub kind: OperationKind,
    pub outcome: OperationOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transfer {
    pub network: NetworkId,
    pub idempotency_key: IdempotencyKey,
    pub asset: AssetId,
    pub from: AccountId,
    pub to: AccountId,
    pub amount: Balance,
    pub fee: Balance,
    pub nonce: Nonce,
    pub valid_until_height: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferItem {
    pub to: AccountId,
    pub amount: Balance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferBatch {
    pub network: NetworkId,
    pub idempotency_key: IdempotencyKey,
    pub asset: AssetId,
    pub from: AccountId,
    pub items: Vec<TransferItem>,
    pub fee: Balance,
    pub nonce: Nonce,
    pub valid_until_height: u64,
}
