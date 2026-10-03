use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

use crate::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, NetworkConfigError,
    SignedNetworkConfig,
};

const CONFIG_DOMAIN: [u8; 16] = *b"net.config.v1.0\0";
const SIGNED_DOMAIN: [u8; 16] = *b"net.signed.v1.0\0";
pub const NETWORK_CONFIG_BYTES: usize = 197;
pub const SIGNED_NETWORK_CONFIG_BYTES: usize = 16 + NETWORK_CONFIG_BYTES + 64;

#[derive(Clone, Copy, Debug, Default)]
pub struct NetworkConfigCodec;

impl NetworkConfigCodec {
    #[must_use]
    pub fn encode(config: NetworkConfig) -> [u8; NETWORK_CONFIG_BYTES] {
        let mut output = [0_u8; NETWORK_CONFIG_BYTES];
        let checkpoint = config.genesis_checkpoint();
        output[..16].copy_from_slice(&CONFIG_DOMAIN);
        output[16..48].copy_from_slice(config.network().as_bytes());
        output[48..56].copy_from_slice(&checkpoint.height.to_be_bytes());
        output[56..88].copy_from_slice(checkpoint.block_hash.as_bytes());
        output[88..120].copy_from_slice(checkpoint.state_root.as_bytes());
        output[120..152].copy_from_slice(checkpoint.validator_set_hash.as_bytes());
        output[152..156].copy_from_slice(&config.protocol_version().to_be_bytes());
        output[156..188].copy_from_slice(config.configuration_key().as_bytes());
        if let Some(height) = config.state_commitment_policy().authenticated_from_height() {
            output[188] = 1;
            output[189..197].copy_from_slice(&height.to_be_bytes());
        }
        output
    }

    /// Decodes and validates one canonical fixed-size configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong length/domain, non-canonical policy or
    /// invalid network configuration.
    pub fn decode(input: &[u8]) -> Result<NetworkConfig, NetworkConfigError> {
        if input.len() != NETWORK_CONFIG_BYTES {
            return Err(NetworkConfigError::InvalidLength);
        }
        if input[..16] != CONFIG_DOMAIN {
            return Err(NetworkConfigError::InvalidDomain);
        }
        let activation = match input[188] {
            0 if input[189..197] == [0; 8] => StateCommitmentPolicy::canonical_state_v1(),
            1 => StateCommitmentPolicy::authenticated_state_v1_from(u64::from_be_bytes(array(
                &input[189..197],
            )?)),
            _ => return Err(NetworkConfigError::UnsupportedValue),
        };
        NetworkConfig::new(
            NetworkId::new(array(&input[16..48])?),
            FinalizedCheckpoint {
                height: u64::from_be_bytes(array(&input[48..56])?),
                block_hash: BlockHash::new(array(&input[56..88])?),
                state_root: StateRoot::new(array(&input[88..120])?),
                validator_set_hash: ValidatorSetHash::new(array(&input[120..152])?),
            },
            u32::from_be_bytes(array(&input[152..156])?),
            ConfigurationPublicKey::new(array(&input[156..188])?),
            activation,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SignedNetworkConfigCodec;

impl SignedNetworkConfigCodec {
    #[must_use]
    pub fn encode(config: SignedNetworkConfig) -> [u8; SIGNED_NETWORK_CONFIG_BYTES] {
        let mut output = [0_u8; SIGNED_NETWORK_CONFIG_BYTES];
        output[..16].copy_from_slice(&SIGNED_DOMAIN);
        output[16..16 + NETWORK_CONFIG_BYTES]
            .copy_from_slice(&NetworkConfigCodec::encode(config.config()));
        output[16 + NETWORK_CONFIG_BYTES..].copy_from_slice(config.signature().as_bytes());
        output
    }

    /// Decodes one signed configuration envelope.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong length/domain or invalid inner config.
    pub fn decode(input: &[u8]) -> Result<SignedNetworkConfig, NetworkConfigError> {
        if input.len() != SIGNED_NETWORK_CONFIG_BYTES {
            return Err(NetworkConfigError::InvalidLength);
        }
        if input[..16] != SIGNED_DOMAIN {
            return Err(NetworkConfigError::InvalidDomain);
        }
        let config = NetworkConfigCodec::decode(&input[16..16 + NETWORK_CONFIG_BYTES])?;
        let signature = ConfigurationSignature::new(array(
            &input[16 + NETWORK_CONFIG_BYTES..SIGNED_NETWORK_CONFIG_BYTES],
        )?);
        Ok(SignedNetworkConfig::new(config, signature))
    }
}

fn array<const SIZE: usize>(input: &[u8]) -> Result<[u8; SIZE], NetworkConfigError> {
    input
        .try_into()
        .map_err(|_| NetworkConfigError::InvalidLength)
}
