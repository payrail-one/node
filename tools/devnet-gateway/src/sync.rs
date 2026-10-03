use ledger_core::NetworkId;
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};
use tail_sync_core::{FinalizedTailBlock, MAX_TAIL_PAYLOAD_BYTES};

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::{SyncBlockView, SyncCheckpointView},
};

pub(crate) fn checkpoint_view(checkpoint: FinalizedCheckpoint) -> SyncCheckpointView {
    SyncCheckpointView {
        height: checkpoint.height.to_string(),
        block_hash: encode_hex(checkpoint.block_hash.as_bytes()),
        state_root: encode_hex(checkpoint.state_root.as_bytes()),
        validator_set_hash: encode_hex(checkpoint.validator_set_hash.as_bytes()),
    }
}

pub(crate) fn decode_block(view: &SyncBlockView) -> Result<FinalizedTailBlock, DevnetError> {
    let network = NetworkId::new(decode_array(&view.network_id)?);
    let parent = decode_checkpoint(&view.parent)?;
    let checkpoint = decode_checkpoint(&view.checkpoint)?;
    if checkpoint.height
        != parent
            .height
            .checked_add(1)
            .ok_or(DevnetError::InvalidSyncBlock)?
    {
        return Err(DevnetError::InvalidSyncBlock);
    }
    Ok(FinalizedTailBlock {
        network,
        parent_hash: parent.block_hash,
        checkpoint,
        payload: decode_bounded_hex(&view.payload, MAX_TAIL_PAYLOAD_BYTES)?,
        finality_proof: decode_bounded_hex(
            &view.finality_proof,
            state_sync_core::MAX_FINALITY_PROOF_BYTES,
        )?,
    })
}

pub(crate) fn decode_checkpoint(
    view: &SyncCheckpointView,
) -> Result<FinalizedCheckpoint, DevnetError> {
    Ok(FinalizedCheckpoint {
        height: view
            .height
            .parse::<u64>()
            .map_err(|_| DevnetError::InvalidSyncBlock)?,
        block_hash: BlockHash::new(decode_array(&view.block_hash)?),
        state_root: StateRoot::new(decode_array(&view.state_root)?),
        validator_set_hash: ValidatorSetHash::new(decode_array(&view.validator_set_hash)?),
    })
}

fn decode_array(value: &str) -> Result<[u8; 32], DevnetError> {
    decode_bounded_hex(value, 32)?
        .try_into()
        .map_err(|_| DevnetError::InvalidSyncBlock)
}
