use ledger_core::NetworkId;
use state_sync_core::{FinalityProofVerifier, FinalizedCheckpoint};

use crate::{
    FinalityCertificate, FinalityCertificateCodec, FinalityError, FinalitySignatureVerifier,
    MAX_VALIDATORS, ValidatorSet, VerifiedFinality,
};

#[derive(Debug)]
pub struct FinalityVerifier<V> {
    validator_set: ValidatorSet,
    signature_verifier: V,
}

impl<V: FinalitySignatureVerifier> FinalityVerifier<V> {
    #[must_use]
    pub const fn new(validator_set: ValidatorSet, signature_verifier: V) -> Self {
        Self {
            validator_set,
            signature_verifier,
        }
    }

    /// Verifies checkpoint identity, canonical signer set, every signature and
    /// strictly greater than two-thirds of configured validator weight.
    ///
    /// # Errors
    ///
    /// Returns an error for any context mismatch, unknown/duplicate signer,
    /// invalid signature, arithmetic overflow or insufficient quorum.
    pub fn verify_certificate(
        &self,
        expected_network: NetworkId,
        expected_checkpoint: FinalizedCheckpoint,
        certificate: &FinalityCertificate,
    ) -> Result<VerifiedFinality, FinalityError> {
        if expected_network != self.validator_set.network()
            || certificate.network != expected_network
        {
            return Err(FinalityError::WrongNetwork);
        }
        if certificate.membership_epoch != self.validator_set.membership_epoch() {
            return Err(FinalityError::MembershipEpochMismatch);
        }
        if certificate.checkpoint.validator_set_hash != self.validator_set.hash() {
            return Err(FinalityError::ValidatorSetMismatch);
        }
        if certificate.checkpoint != expected_checkpoint {
            return Err(FinalityError::CheckpointMismatch);
        }
        if certificate.signatures.is_empty() {
            return Err(FinalityError::EmptyCertificate);
        }
        if certificate.signatures.len() > MAX_VALIDATORS {
            return Err(FinalityError::TooManySignatures);
        }
        if certificate
            .signatures
            .windows(2)
            .any(|pair| pair[0].node >= pair[1].node)
        {
            return Err(FinalityError::NonCanonicalSignatureOrder);
        }
        let message = certificate.signing_message();
        let mut signed_weight = 0_u64;
        for signature in &certificate.signatures {
            let validator = self
                .validator_set
                .validator(signature.node)
                .ok_or(FinalityError::UnknownValidator)?;
            if !self.signature_verifier.verify(
                validator.consensus_key,
                &message,
                signature.signature,
            ) {
                return Err(FinalityError::InvalidSignature);
            }
            signed_weight = signed_weight
                .checked_add(validator.weight)
                .ok_or(FinalityError::SignedWeightOverflow)?;
        }
        if signed_weight < self.validator_set.quorum_weight() {
            return Err(FinalityError::InsufficientQuorum);
        }
        Ok(VerifiedFinality {
            validator_set: self.validator_set.hash(),
            signed_weight,
            quorum_weight: self.validator_set.quorum_weight(),
        })
    }
}

impl<V: FinalitySignatureVerifier> FinalityProofVerifier for FinalityVerifier<V> {
    fn verify(&self, network: NetworkId, checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        FinalityCertificateCodec::decode(proof)
            .and_then(|certificate| self.verify_certificate(network, checkpoint, &certificate))
            .is_ok()
    }
}
