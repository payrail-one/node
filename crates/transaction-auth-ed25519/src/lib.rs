#![forbid(unsafe_code)]

use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, VerifyingKey, verify_batch};
use ledger_core::{AccountId, SignatureBytes, SignatureVerification, SignatureVerifier};

/// Strict Ed25519 verifier for ledger authorization messages.
///
/// Account identifiers at this boundary are canonical Ed25519 public keys.
/// Invalid encodings, weak keys and non-canonical signatures are rejected.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ed25519Verifier;

impl SignatureVerifier for Ed25519Verifier {
    fn verify(&self, signer: AccountId, message: &[u8], signature: SignatureBytes) -> bool {
        let Ok(verifying_key) = VerifyingKey::from_bytes(signer.as_bytes()) else {
            return false;
        };
        let signature = Signature::from_bytes(signature.as_bytes());
        verifying_key.verify_strict(message, &signature).is_ok()
    }

    fn verify_batch(&self, verifications: &[SignatureVerification<'_>]) -> bool {
        if verifications.len() < 2 {
            return verifications.iter().all(|verification| {
                self.verify(
                    verification.signer(),
                    verification.message(),
                    verification.signature(),
                )
            });
        }
        let mut keys = Vec::with_capacity(verifications.len());
        let mut signatures = Vec::with_capacity(verifications.len());
        for verification in verifications {
            let Ok(key) = VerifyingKey::from_bytes(verification.signer().as_bytes()) else {
                return false;
            };
            if key.is_weak() {
                return false;
            }
            let signature = Signature::from_bytes(verification.signature().as_bytes());
            let Some(point) = CompressedEdwardsY(*signature.r_bytes()).decompress() else {
                return false;
            };
            if point.is_small_order() {
                return false;
            }
            keys.push(key);
            signatures.push(signature);
        }
        let messages = verifications
            .iter()
            .map(|verification| verification.message())
            .collect::<Vec<_>>();
        verify_batch(&messages, &signatures, &keys).is_ok()
    }
}
