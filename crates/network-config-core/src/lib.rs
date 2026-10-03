#![forbid(unsafe_code)]

mod codec;
mod error;
mod types;
mod verification;

pub use codec::{NetworkConfigCodec, SIGNED_NETWORK_CONFIG_BYTES, SignedNetworkConfigCodec};
pub use error::NetworkConfigError;
pub use types::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, SignedNetworkConfig,
    VerifiedNetworkConfig,
};
pub use verification::NetworkConfigSignatureVerifier;
