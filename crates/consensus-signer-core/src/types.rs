use ledger_core::NetworkId;
use network_membership_core::ConsensusPublicKey;
use state_sync_core::BlockHash;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum VoteStage {
    Proposal,
    Prevote,
    Precommit,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VoteSlot {
    pub network: NetworkId,
    pub set_id: u64,
    pub height: u64,
    pub round: u64,
    pub stage: VoteStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoteIntent {
    pub slot: VoteSlot,
    pub target_hash: BlockHash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reservation {
    New,
    ExistingSame,
    Conflict,
}

pub trait SigningJournal {
    type Error;

    /// Atomically and durably reserves one vote target for a consensus slot.
    ///
    /// # Errors
    ///
    /// Returns an implementation error if durable compare-and-set fails.
    fn reserve(&mut self, intent: VoteIntent) -> Result<Reservation, Self::Error>;
}

pub trait VotePayloadEncoder {
    type Error;

    /// Encodes the exact consensus-engine payload for the approved vote intent.
    ///
    /// # Errors
    ///
    /// Returns an adapter error when the payload cannot be encoded.
    fn encode(&self, intent: VoteIntent) -> Result<Vec<u8>, Self::Error>;
}

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

pub trait VoteSigner {
    type Error;

    /// Signs an engine-encoded payload using the configured consensus key.
    ///
    /// # Errors
    ///
    /// Returns an HSM/remote-signer error without releasing the reservation.
    fn sign(
        &self,
        public_key: ConsensusPublicKey,
        payload: &[u8],
    ) -> Result<ConsensusSignature, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardedSignature {
    pub signature: ConsensusSignature,
    pub repeated_request: bool,
}
