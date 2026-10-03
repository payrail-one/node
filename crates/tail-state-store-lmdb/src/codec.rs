use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use sha2::{Digest, Sha256};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

use crate::LmdbStateStoreError;
use crate::StoredAuthenticatedState;

const CURRENT_MAGIC: [u8; 8] = *b"TLCUR001";
const BLOCK_MAGIC: [u8; 8] = *b"TLBLK001";
const STATE_MAGIC: [u8; 8] = *b"TLSTA001";
const AUTHENTICATED_STATE_MAGIC: [u8; 8] = *b"TLJMT001";
const COMMITMENT_POLICY_MAGIC: [u8; 8] = *b"TLCMT001";
const CHECKPOINT_LENGTH: usize = 104;
const CHECKSUM_LENGTH: usize = 32;
const CURRENT_LENGTH: usize = 8 + CHECKPOINT_LENGTH + CHECKSUM_LENGTH;
const BLOCK_BODY_LENGTH: usize = 8 + 32 + CHECKPOINT_LENGTH + CHECKPOINT_LENGTH + 32;
const BLOCK_LENGTH: usize = BLOCK_BODY_LENGTH + CHECKSUM_LENGTH;
const STATE_HEADER_LENGTH: usize = 16;
const AUTHENTICATED_STATE_BODY_LENGTH: usize = 8 + 8 + 8 + 8 + 32;
const AUTHENTICATED_STATE_LENGTH: usize = AUTHENTICATED_STATE_BODY_LENGTH + CHECKSUM_LENGTH;
const COMMITMENT_POLICY_BODY_LENGTH: usize = 24;
const COMMITMENT_POLICY_LENGTH: usize = COMMITMENT_POLICY_BODY_LENGTH + CHECKSUM_LENGTH;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BlockRecord {
    pub network: NetworkId,
    pub previous: FinalizedCheckpoint,
    pub checkpoint: FinalizedCheckpoint,
    pub payload_hash: [u8; 32],
}

pub(crate) fn encode_current(checkpoint: FinalizedCheckpoint) -> [u8; CURRENT_LENGTH] {
    let mut output = [0_u8; CURRENT_LENGTH];
    output[..8].copy_from_slice(&CURRENT_MAGIC);
    encode_checkpoint(&mut output[8..8 + CHECKPOINT_LENGTH], checkpoint);
    let digest: [u8; 32] = Sha256::digest(&output[..8 + CHECKPOINT_LENGTH]).into();
    output[8 + CHECKPOINT_LENGTH..].copy_from_slice(&digest);
    output
}

pub(crate) fn decode_current(input: &[u8]) -> Result<FinalizedCheckpoint, LmdbStateStoreError> {
    if input.len() != CURRENT_LENGTH || input[..8] != CURRENT_MAGIC {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    verify_checksum(input, 8 + CHECKPOINT_LENGTH)?;
    decode_checkpoint(&input[8..8 + CHECKPOINT_LENGTH])
}

pub(crate) fn encode_block(record: &BlockRecord) -> [u8; BLOCK_LENGTH] {
    let mut output = [0_u8; BLOCK_LENGTH];
    output[..8].copy_from_slice(&BLOCK_MAGIC);
    output[8..40].copy_from_slice(record.network.as_bytes());
    encode_checkpoint(&mut output[40..144], record.previous);
    encode_checkpoint(&mut output[144..248], record.checkpoint);
    output[248..280].copy_from_slice(&record.payload_hash);
    let digest: [u8; 32] = Sha256::digest(&output[..BLOCK_BODY_LENGTH]).into();
    output[BLOCK_BODY_LENGTH..].copy_from_slice(&digest);
    output
}

pub(crate) fn decode_block(input: &[u8]) -> Result<BlockRecord, LmdbStateStoreError> {
    if input.len() != BLOCK_LENGTH || input[..8] != BLOCK_MAGIC {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    verify_checksum(input, BLOCK_BODY_LENGTH)?;
    Ok(BlockRecord {
        network: NetworkId::new(array(&input[8..40])?),
        previous: decode_checkpoint(&input[40..144])?,
        checkpoint: decode_checkpoint(&input[144..248])?,
        payload_hash: array(&input[248..280])?,
    })
}

pub(crate) fn encode_state(state: &[u8]) -> Result<Vec<u8>, LmdbStateStoreError> {
    let length = u64::try_from(state.len()).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
    let capacity = STATE_HEADER_LENGTH
        .checked_add(state.len())
        .and_then(|value| value.checked_add(CHECKSUM_LENGTH))
        .ok_or(LmdbStateStoreError::CorruptRecord)?;
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&STATE_MAGIC);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(state);
    let digest: [u8; 32] = Sha256::digest(&output).into();
    output.extend_from_slice(&digest);
    Ok(output)
}

pub(crate) fn decode_state(input: &[u8]) -> Result<Vec<u8>, LmdbStateStoreError> {
    if input.len() < STATE_HEADER_LENGTH + CHECKSUM_LENGTH || input[..8] != STATE_MAGIC {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    let declared = usize::try_from(u64::from_be_bytes(array(&input[8..16])?))
        .map_err(|_| LmdbStateStoreError::CorruptRecord)?;
    let body_end = STATE_HEADER_LENGTH
        .checked_add(declared)
        .ok_or(LmdbStateStoreError::CorruptRecord)?;
    if body_end + CHECKSUM_LENGTH != input.len() {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    verify_checksum(input, body_end)?;
    Ok(input[STATE_HEADER_LENGTH..body_end].to_vec())
}

pub(crate) fn payload_hash(payload: &[u8]) -> [u8; 32] {
    Sha256::digest(payload).into()
}

pub(crate) fn encode_authenticated_state(
    state: StoredAuthenticatedState,
) -> [u8; AUTHENTICATED_STATE_LENGTH] {
    let mut output = [0_u8; AUTHENTICATED_STATE_LENGTH];
    output[..8].copy_from_slice(&AUTHENTICATED_STATE_MAGIC);
    output[8..16].copy_from_slice(&state.base_height.to_be_bytes());
    output[16..24].copy_from_slice(&state.latest_height.to_be_bytes());
    output[24..32].copy_from_slice(&state.tree_version.to_be_bytes());
    output[32..64].copy_from_slice(state.root.as_bytes());
    let digest: [u8; 32] = Sha256::digest(&output[..AUTHENTICATED_STATE_BODY_LENGTH]).into();
    output[AUTHENTICATED_STATE_BODY_LENGTH..].copy_from_slice(&digest);
    output
}

pub(crate) fn decode_authenticated_state(
    input: &[u8],
) -> Result<StoredAuthenticatedState, LmdbStateStoreError> {
    if input.len() != AUTHENTICATED_STATE_LENGTH || input[..8] != AUTHENTICATED_STATE_MAGIC {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    verify_checksum(input, AUTHENTICATED_STATE_BODY_LENGTH)?;
    let state = StoredAuthenticatedState {
        base_height: u64::from_be_bytes(array(&input[8..16])?),
        latest_height: u64::from_be_bytes(array(&input[16..24])?),
        tree_version: u64::from_be_bytes(array(&input[24..32])?),
        root: StateRoot::new(array(&input[32..64])?),
    };
    let expected_version = state
        .latest_height
        .checked_sub(state.base_height)
        .ok_or(LmdbStateStoreError::CorruptRecord)?;
    if state.tree_version != expected_version {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Ok(state)
}

pub(crate) fn encode_commitment_policy(
    policy: StateCommitmentPolicy,
) -> [u8; COMMITMENT_POLICY_LENGTH] {
    let mut output = [0_u8; COMMITMENT_POLICY_LENGTH];
    output[..8].copy_from_slice(&COMMITMENT_POLICY_MAGIC);
    if let Some(height) = policy.authenticated_from_height() {
        output[8] = 1;
        output[16..24].copy_from_slice(&height.to_be_bytes());
    }
    let digest: [u8; 32] = Sha256::digest(&output[..COMMITMENT_POLICY_BODY_LENGTH]).into();
    output[COMMITMENT_POLICY_BODY_LENGTH..].copy_from_slice(&digest);
    output
}

pub(crate) fn decode_commitment_policy(
    input: &[u8],
) -> Result<StateCommitmentPolicy, LmdbStateStoreError> {
    if input.len() != COMMITMENT_POLICY_LENGTH || input[..8] != COMMITMENT_POLICY_MAGIC {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    verify_checksum(input, COMMITMENT_POLICY_BODY_LENGTH)?;
    if input[9..16].iter().any(|byte| *byte != 0) {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    let height = u64::from_be_bytes(array(&input[16..24])?);
    match input[8] {
        0 if height == 0 => Ok(StateCommitmentPolicy::canonical_state_v1()),
        1 => Ok(StateCommitmentPolicy::authenticated_state_v1_from(height)),
        _ => Err(LmdbStateStoreError::CorruptRecord),
    }
}

fn encode_checkpoint(output: &mut [u8], checkpoint: FinalizedCheckpoint) {
    output[..8].copy_from_slice(&checkpoint.height.to_be_bytes());
    output[8..40].copy_from_slice(checkpoint.block_hash.as_bytes());
    output[40..72].copy_from_slice(checkpoint.state_root.as_bytes());
    output[72..104].copy_from_slice(checkpoint.validator_set_hash.as_bytes());
}

fn decode_checkpoint(input: &[u8]) -> Result<FinalizedCheckpoint, LmdbStateStoreError> {
    if input.len() != CHECKPOINT_LENGTH {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Ok(FinalizedCheckpoint {
        height: u64::from_be_bytes(array(&input[..8])?),
        block_hash: BlockHash::new(array(&input[8..40])?),
        state_root: StateRoot::new(array(&input[40..72])?),
        validator_set_hash: ValidatorSetHash::new(array(&input[72..104])?),
    })
}

fn verify_checksum(input: &[u8], checksum_offset: usize) -> Result<(), LmdbStateStoreError> {
    let expected: [u8; 32] = Sha256::digest(&input[..checksum_offset]).into();
    if input[checksum_offset..] == expected {
        Ok(())
    } else {
        Err(LmdbStateStoreError::CorruptRecord)
    }
}

fn array<const N: usize>(input: &[u8]) -> Result<[u8; N], LmdbStateStoreError> {
    input
        .try_into()
        .map_err(|_| LmdbStateStoreError::CorruptRecord)
}
