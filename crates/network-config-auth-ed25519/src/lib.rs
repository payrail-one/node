#![forbid(unsafe_code)]

use ed25519_dalek::{Signature, VerifyingKey};
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfigSignatureVerifier,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct Ed25519NetworkConfigVerifier;

impl NetworkConfigSignatureVerifier for Ed25519NetworkConfigVerifier {
    fn verify(
        &self,
        public_key: ConfigurationPublicKey,
        message: &[u8],
        signature: ConfigurationSignature,
    ) -> bool {
        let Ok(key) = VerifyingKey::from_bytes(public_key.as_bytes()) else {
            return false;
        };
        key.verify_strict(message, &Signature::from_bytes(signature.as_bytes()))
            .is_ok()
    }
}
