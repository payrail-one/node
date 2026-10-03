use crate::{AccountId, AssetId, Balance, ContractId, IdempotencyKey, NetworkId, Nonce};

pub const MAX_CONTRACT_CODE_BYTES: usize = 16 * 1024;
pub const MAX_CONTRACT_ARGS_BYTES: usize = 4 * 1024;
pub const MAX_CONTRACT_EXECUTION_UNITS: u64 = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractDeploy {
    pub network: NetworkId,
    pub idempotency_key: IdempotencyKey,
    pub asset: AssetId,
    pub owner: AccountId,
    pub salt: [u8; 32],
    pub code: Vec<u8>,
    pub fee: Balance,
    pub nonce: Nonce,
    pub valid_until_height: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractCall {
    pub network: NetworkId,
    pub idempotency_key: IdempotencyKey,
    pub asset: AssetId,
    pub caller: AccountId,
    pub contract: ContractId,
    pub entrypoint: String,
    pub args: Vec<u8>,
    pub attached_amount: Balance,
    pub fee: Balance,
    pub execution_limit: u64,
    pub nonce: Nonce,
    pub valid_until_height: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractRecord {
    pub id: ContractId,
    pub owner: AccountId,
    pub code: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractStateEntry {
    pub contract: ContractId,
    pub key: Vec<u8>,
    pub value: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEventRecord {
    pub contract: ContractId,
    pub topic: Vec<u8>,
}
