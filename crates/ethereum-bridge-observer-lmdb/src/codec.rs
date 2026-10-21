use ethereum_bridge_core::{
    EvmAddress, EvmChainId, EvmHash, EvmObserverCheckpoint, EvmObserverScope,
    PreparedEvmObserverBlock,
};
use external_asset_core::{
    DepositId, FinalityEvidence, OriginAssetId, OriginDeposit, OriginNetworkId, OriginTransactionId,
};
use ledger_core::{AccountId, AssetId};
use sha2::{Digest, Sha256};

use crate::{EvmObserverStoreError, StoredEvmDeposit, types::LastBlockRecord};

const VERSION: u8 = 1;
const SCOPE_LENGTH: usize = 69;
const CHECKPOINT_LENGTH: usize = 41;
const LAST_BLOCK_LENGTH: usize = 73;
const DEPOSIT_LENGTH: usize = 252;

pub(crate) fn encode_scope(scope: EvmObserverScope) -> [u8; SCOPE_LENGTH] {
    let mut output = [0_u8; SCOPE_LENGTH];
    output[0] = VERSION;
    output[1..9].copy_from_slice(&scope.chain_id.get().to_be_bytes());
    output[9..29].copy_from_slice(scope.bridge_contract.as_bytes());
    output[29..37].copy_from_slice(&scope.first_block.to_be_bytes());
    output[37..69].copy_from_slice(scope.first_parent_hash.as_bytes());
    output
}

pub(crate) fn decode_scope(bytes: &[u8]) -> Result<EvmObserverScope, EvmObserverStoreError> {
    if bytes.len() != SCOPE_LENGTH || bytes[0] != VERSION {
        return Err(EvmObserverStoreError::CorruptRecord);
    }
    let chain_id =
        EvmChainId::new(read_u64(&bytes[1..9])?).ok_or(EvmObserverStoreError::CorruptRecord)?;
    EvmObserverScope::new(
        chain_id,
        EvmAddress::new(read_array(&bytes[9..29])?),
        read_u64(&bytes[29..37])?,
        EvmHash::new(read_array(&bytes[37..69])?),
    )
    .map_err(Into::into)
}

pub(crate) fn encode_checkpoint(checkpoint: EvmObserverCheckpoint) -> [u8; CHECKPOINT_LENGTH] {
    let mut output = [0_u8; CHECKPOINT_LENGTH];
    output[0] = VERSION;
    output[1..9].copy_from_slice(&checkpoint.next_block.to_be_bytes());
    output[9..41].copy_from_slice(checkpoint.parent_hash.as_bytes());
    output
}

pub(crate) fn decode_checkpoint(
    bytes: &[u8],
) -> Result<EvmObserverCheckpoint, EvmObserverStoreError> {
    if bytes.len() != CHECKPOINT_LENGTH || bytes[0] != VERSION {
        return Err(EvmObserverStoreError::CorruptRecord);
    }
    Ok(EvmObserverCheckpoint {
        next_block: read_u64(&bytes[1..9])?,
        parent_hash: EvmHash::new(read_array(&bytes[9..41])?),
    })
}

pub(crate) fn encode_last_block(record: LastBlockRecord) -> [u8; LAST_BLOCK_LENGTH] {
    let mut output = [0_u8; LAST_BLOCK_LENGTH];
    output[0] = VERSION;
    output[1..9].copy_from_slice(&record.number.to_be_bytes());
    output[9..41].copy_from_slice(record.hash.as_bytes());
    output[41..73].copy_from_slice(&record.digest);
    output
}

pub(crate) fn decode_last_block(bytes: &[u8]) -> Result<LastBlockRecord, EvmObserverStoreError> {
    if bytes.len() != LAST_BLOCK_LENGTH || bytes[0] != VERSION {
        return Err(EvmObserverStoreError::CorruptRecord);
    }
    Ok(LastBlockRecord {
        number: read_u64(&bytes[1..9])?,
        hash: EvmHash::new(read_array(&bytes[9..41])?),
        digest: read_array(&bytes[41..73])?,
    })
}

pub(crate) fn encode_deposit(
    deposit: &ethereum_bridge_core::VerifiedEvmDeposit,
) -> [u8; DEPOSIT_LENGTH] {
    let claim = deposit.claim();
    let evidence = deposit.evidence();
    let mut output = [0_u8; DEPOSIT_LENGTH];
    output[..32].copy_from_slice(claim.asset.as_bytes());
    output[32..64].copy_from_slice(claim.origin_network.as_bytes());
    output[64..96].copy_from_slice(claim.origin_asset.as_bytes());
    output[96..128].copy_from_slice(claim.transaction.as_bytes());
    output[128..132].copy_from_slice(&claim.event_index.to_be_bytes());
    output[132..164].copy_from_slice(claim.recipient.as_bytes());
    output[164..180].copy_from_slice(&claim.amount.to_be_bytes());
    output[180..188].copy_from_slice(&evidence.block_height.to_be_bytes());
    output[188..220].copy_from_slice(&evidence.block_hash);
    output[220..252].copy_from_slice(&evidence.proof_hash);
    output
}

pub(crate) fn decode_deposit(
    id: DepositId,
    bytes: &[u8],
) -> Result<StoredEvmDeposit, EvmObserverStoreError> {
    if bytes.len() != DEPOSIT_LENGTH {
        return Err(EvmObserverStoreError::CorruptRecord);
    }
    let claim = OriginDeposit {
        asset: AssetId::new(read_array(&bytes[..32])?),
        origin_network: OriginNetworkId::new(read_array(&bytes[32..64])?),
        origin_asset: OriginAssetId::new(read_array(&bytes[64..96])?),
        transaction: OriginTransactionId::new(read_array(&bytes[96..128])?),
        event_index: read_u32(&bytes[128..132])?,
        recipient: AccountId::new(read_array(&bytes[132..164])?),
        amount: read_u128(&bytes[164..180])?,
    };
    let evidence = FinalityEvidence {
        block_height: read_u64(&bytes[180..188])?,
        block_hash: read_array(&bytes[188..220])?,
        proof_hash: read_array(&bytes[220..252])?,
    };
    if claim.amount == 0
        || evidence.proof_hash.iter().all(|byte| *byte == 0)
        || DepositId::derive(&claim) != id
    {
        return Err(EvmObserverStoreError::CorruptRecord);
    }
    Ok(StoredEvmDeposit {
        id,
        claim,
        evidence,
    })
}

pub(crate) fn block_digest(block: &PreparedEvmObserverBlock) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"payrail.ethereum.observer-block\0");
    hasher.update(encode_scope(block.scope()));
    hasher.update(block.finalized().number.to_be_bytes());
    hasher.update(block.finalized().hash.as_bytes());
    hasher.update(block.finalized().proof_hash);
    hasher.update(block.previous().parent_hash.as_bytes());
    let deposit_count = u64::try_from(block.deposits().len()).unwrap_or(u64::MAX);
    hasher.update(deposit_count.to_be_bytes());
    for deposit in block.deposits() {
        hasher.update(DepositId::derive(&deposit.claim()).as_bytes());
        hasher.update(encode_deposit(deposit));
    }
    hasher.finalize().into()
}

fn read_array<const SIZE: usize>(bytes: &[u8]) -> Result<[u8; SIZE], EvmObserverStoreError> {
    bytes
        .try_into()
        .map_err(|_| EvmObserverStoreError::CorruptRecord)
}

fn read_u32(bytes: &[u8]) -> Result<u32, EvmObserverStoreError> {
    read_array(bytes).map(u32::from_be_bytes)
}

fn read_u64(bytes: &[u8]) -> Result<u64, EvmObserverStoreError> {
    read_array(bytes).map(u64::from_be_bytes)
}

fn read_u128(bytes: &[u8]) -> Result<u128, EvmObserverStoreError> {
    read_array(bytes).map(u128::from_be_bytes)
}
