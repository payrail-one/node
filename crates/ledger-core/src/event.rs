use crate::{
    AccountId, AccountStatus, AssetId, AssetStatus, Balance, IdempotencyKey, Nonce, OperationId,
    OperationKind, TransferItem,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    AssetRegistered {
        asset: AssetId,
    },
    StatusChanged {
        asset: AssetId,
        status: AssetStatus,
    },
    BackingObserved {
        asset: AssetId,
        amount: Balance,
        deficit: bool,
    },
    Minted {
        asset: AssetId,
        to: AccountId,
        amount: Balance,
    },
    Burned {
        asset: AssetId,
        from: AccountId,
        amount: Balance,
    },
    Transferred {
        operation_id: OperationId,
        idempotency_key: IdempotencyKey,
        asset: AssetId,
        from: AccountId,
        to: AccountId,
        fee_payer: AccountId,
        amount: Balance,
        fee: Balance,
        nonce: Nonce,
    },
    BatchTransferred {
        operation_id: OperationId,
        idempotency_key: IdempotencyKey,
        asset: AssetId,
        from: AccountId,
        fee_payer: AccountId,
        items: Vec<TransferItem>,
        fee: Balance,
        nonce: Nonce,
    },
    OperationExpired {
        operation_id: OperationId,
        idempotency_key: IdempotencyKey,
        account: AccountId,
        nonce: Nonce,
        kind: OperationKind,
        valid_until_height: u64,
        finalized_at_height: u64,
    },
    AccountStatusChanged {
        asset: AssetId,
        account: AccountId,
        status: AccountStatus,
    },
}
