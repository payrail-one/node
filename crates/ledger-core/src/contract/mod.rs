mod bytecode;
mod types;

pub(crate) use bytecode::{execute, validate_code};
pub use types::{
    ContractCall, ContractDeploy, ContractEventRecord, ContractRecord, ContractStateEntry,
    MAX_CONTRACT_ARGS_BYTES, MAX_CONTRACT_CODE_BYTES, MAX_CONTRACT_EXECUTION_UNITS,
};

use sha2::{Digest, Sha256};

use crate::ContractId;

const CONTRACT_ID_DOMAIN: &[u8] = b"payrail.contract-id.v1\0";

#[must_use]
pub fn contract_id(deploy: &ContractDeploy) -> ContractId {
    let mut hasher = Sha256::new();
    hasher.update(CONTRACT_ID_DOMAIN);
    hasher.update(deploy.network.as_bytes());
    hasher.update(deploy.owner.as_bytes());
    hasher.update(deploy.salt);
    hasher.update(Sha256::digest(&deploy.code));
    ContractId::new(hasher.finalize().into())
}
