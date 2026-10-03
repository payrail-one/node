#![forbid(unsafe_code)]

use ed25519_dalek::{Signature, VerifyingKey};
use finality_proof_core::{ConsensusSignature, FinalitySignatureVerifier};
use network_membership_core::ConsensusPublicKey;

#[derive(Clone, Copy, Debug, Default)]
pub struct Ed25519FinalityVerifier;

impl FinalitySignatureVerifier for Ed25519FinalityVerifier {
    fn verify(
        &self,
        public_key: ConsensusPublicKey,
        message: &[u8],
        signature: ConsensusSignature,
    ) -> bool {
        let Ok(key) = VerifyingKey::from_bytes(public_key.as_bytes()) else {
            return false;
        };
        let signature = Signature::from_bytes(signature.as_bytes());
        key.verify_strict(message, &signature).is_ok()
    }
}
