use ledger_core::NetworkId;
use network_membership_core::ConsensusPublicKey;

use crate::{
    ConsensusSignerError, GuardedSignature, Reservation, SigningJournal, VoteIntent,
    VotePayloadEncoder, VoteSigner,
};

#[derive(Debug)]
pub struct ConsensusSigningGuard<J, E, S> {
    network: NetworkId,
    public_key: ConsensusPublicKey,
    journal: J,
    encoder: E,
    signer: S,
}

impl<J, E, S> ConsensusSigningGuard<J, E, S>
where
    J: SigningJournal,
    E: VotePayloadEncoder,
    S: VoteSigner,
{
    #[must_use]
    pub const fn new(
        network: NetworkId,
        public_key: ConsensusPublicKey,
        journal: J,
        encoder: E,
        signer: S,
    ) -> Self {
        Self {
            network,
            public_key,
            journal,
            encoder,
            signer,
        }
    }

    /// Durably reserves a vote before asking the HSM/remote signer to sign it.
    ///
    /// A journal conflict fails closed. A signer failure leaves the reservation
    /// in place, so retry is allowed only for the identical target.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong network, conflicting vote, durable journal
    /// failure, engine payload failure or signer failure.
    pub fn sign(&mut self, intent: VoteIntent) -> Result<GuardedSignature, ConsensusSignerError> {
        if intent.slot.network != self.network {
            return Err(ConsensusSignerError::WrongNetwork);
        }
        let reservation = self
            .journal
            .reserve(intent)
            .map_err(|_| ConsensusSignerError::JournalFailure)?;
        let repeated_request = match reservation {
            Reservation::New => false,
            Reservation::ExistingSame => true,
            Reservation::Conflict => return Err(ConsensusSignerError::ConflictingVote),
        };
        let payload = self
            .encoder
            .encode(intent)
            .map_err(|_| ConsensusSignerError::PayloadEncodingFailure)?;
        let signature = self
            .signer
            .sign(self.public_key, &payload)
            .map_err(|_| ConsensusSignerError::SignerFailure)?;
        Ok(GuardedSignature {
            signature,
            repeated_request,
        })
    }

    #[must_use]
    pub const fn public_key(&self) -> ConsensusPublicKey {
        self.public_key
    }
}
