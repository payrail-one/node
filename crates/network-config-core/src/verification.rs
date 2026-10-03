use crate::{ConfigurationPublicKey, ConfigurationSignature};

pub trait NetworkConfigSignatureVerifier {
    fn verify(
        &self,
        public_key: ConfigurationPublicKey,
        message: &[u8],
        signature: ConfigurationSignature,
    ) -> bool;
}
