use network_membership_core::{ConsensusPublicKey, NodeId};
use state_sync_core::ValidatorSetHash;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsensusSignature([u8; 64]);

impl ConsensusSignature {
    #[must_use]
    pub const fn new(value: [u8; 64]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Validator {
    pub node: NodeId,
    pub consensus_key: ConsensusPublicKey,
    pub weight: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatorSignature {
    pub node: NodeId,
    pub signature: ConsensusSignature,
}

pub trait FinalitySignatureVerifier {
    fn verify(
        &self,
        public_key: ConsensusPublicKey,
        message: &[u8],
        signature: ConsensusSignature,
    ) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedFinality {
    pub validator_set: ValidatorSetHash,
    pub signed_weight: u64,
    pub quorum_weight: u64,
}
