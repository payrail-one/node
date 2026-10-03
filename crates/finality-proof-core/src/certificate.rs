use ledger_core::NetworkId;
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

use crate::ValidatorSignature;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalityCertificate {
    pub network: NetworkId,
    pub membership_epoch: u64,
    pub round: u64,
    pub checkpoint: FinalizedCheckpoint,
    pub signatures: Vec<ValidatorSignature>,
}

impl FinalityCertificate {
    #[must_use]
    pub fn signing_message(&self) -> Vec<u8> {
        signing_message(
            self.network,
            self.membership_epoch,
            self.round,
            self.checkpoint.height,
            self.checkpoint.block_hash,
            self.checkpoint.state_root,
            self.checkpoint.validator_set_hash,
        )
    }
}

pub(crate) fn signing_message(
    network: NetworkId,
    membership_epoch: u64,
    round: u64,
    height: u64,
    block_hash: BlockHash,
    state_root: StateRoot,
    validator_set_hash: ValidatorSetHash,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(173);
    message.extend_from_slice(b"finality.checkpoint\0");
    message.extend_from_slice(network.as_bytes());
    message.extend_from_slice(&membership_epoch.to_be_bytes());
    message.extend_from_slice(&round.to_be_bytes());
    message.extend_from_slice(&height.to_be_bytes());
    message.extend_from_slice(block_hash.as_bytes());
    message.extend_from_slice(state_root.as_bytes());
    message.extend_from_slice(validator_set_hash.as_bytes());
    message
}
