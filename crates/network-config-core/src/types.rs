use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use network_membership_core::ProtocolDigest;
use sha2::{Digest, Sha256};
use state_sync_core::FinalizedCheckpoint;

use crate::{NetworkConfigCodec, NetworkConfigError, NetworkConfigSignatureVerifier};

const DIGEST_DOMAIN: &[u8] = b"network.protocol-config.v1\0";
const AUTHORIZATION_DOMAIN: &[u8] = b"network.protocol-config.authorization.v1\0";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConfigurationPublicKey([u8; 32]);

impl ConfigurationPublicKey {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationSignature([u8; 64]);

impl ConfigurationSignature {
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
pub struct NetworkConfig {
    network: NetworkId,
    genesis_checkpoint: FinalizedCheckpoint,
    protocol_version: u32,
    configuration_key: ConfigurationPublicKey,
    state_commitment_policy: StateCommitmentPolicy,
}

impl NetworkConfig {
    /// Creates a canonical consensus-critical network configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for zero trust anchors, zero checkpoint identifiers,
    /// protocol version zero, or an activation height before genesis.
    pub fn new(
        network: NetworkId,
        genesis_checkpoint: FinalizedCheckpoint,
        protocol_version: u32,
        configuration_key: ConfigurationPublicKey,
        state_commitment_policy: StateCommitmentPolicy,
    ) -> Result<Self, NetworkConfigError> {
        if network.as_bytes() == &[0; 32] {
            return Err(NetworkConfigError::ZeroNetwork);
        }
        if protocol_version == 0 {
            return Err(NetworkConfigError::ZeroProtocolVersion);
        }
        if configuration_key.as_bytes() == &[0; 32] {
            return Err(NetworkConfigError::ZeroConfigurationKey);
        }
        if genesis_checkpoint.block_hash.as_bytes() == &[0; 32] {
            return Err(NetworkConfigError::ZeroGenesisBlockHash);
        }
        if genesis_checkpoint.state_root.as_bytes() == &[0; 32] {
            return Err(NetworkConfigError::ZeroGenesisStateRoot);
        }
        if genesis_checkpoint.validator_set_hash.as_bytes() == &[0; 32] {
            return Err(NetworkConfigError::ZeroValidatorSetHash);
        }
        if state_commitment_policy
            .authenticated_from_height()
            .is_some_and(|height| height < genesis_checkpoint.height)
        {
            return Err(NetworkConfigError::ActivationBeforeGenesis);
        }
        Ok(Self {
            network,
            genesis_checkpoint,
            protocol_version,
            configuration_key,
            state_commitment_policy,
        })
    }

    #[must_use]
    pub const fn network(self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn genesis_checkpoint(self) -> FinalizedCheckpoint {
        self.genesis_checkpoint
    }

    #[must_use]
    pub const fn protocol_version(self) -> u32 {
        self.protocol_version
    }

    #[must_use]
    pub const fn configuration_key(self) -> ConfigurationPublicKey {
        self.configuration_key
    }

    #[must_use]
    pub const fn state_commitment_policy(self) -> StateCommitmentPolicy {
        self.state_commitment_policy
    }

    #[must_use]
    pub fn protocol_digest(self) -> ProtocolDigest {
        let encoded = NetworkConfigCodec::encode(self);
        let mut hasher = Sha256::new();
        hasher.update(DIGEST_DOMAIN);
        hasher.update(encoded);
        ProtocolDigest::new(hasher.finalize().into())
    }

    #[must_use]
    pub fn signing_message(self) -> Vec<u8> {
        let mut message = Vec::with_capacity(AUTHORIZATION_DOMAIN.len() + 32);
        message.extend_from_slice(AUTHORIZATION_DOMAIN);
        message.extend_from_slice(self.protocol_digest().as_bytes());
        message
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedNetworkConfig {
    config: NetworkConfig,
    signature: ConfigurationSignature,
}

impl SignedNetworkConfig {
    #[must_use]
    pub const fn new(config: NetworkConfig, signature: ConfigurationSignature) -> Self {
        Self { config, signature }
    }

    #[must_use]
    pub const fn config(self) -> NetworkConfig {
        self.config
    }

    #[must_use]
    pub const fn signature(self) -> ConfigurationSignature {
        self.signature
    }

    /// Verifies the pinned network, trust anchor and configuration signature.
    ///
    /// # Errors
    ///
    /// Returns an error when identity checks or signature verification fail.
    pub fn verify<V: NetworkConfigSignatureVerifier>(
        self,
        expected_network: NetworkId,
        trusted_key: ConfigurationPublicKey,
        verifier: &V,
    ) -> Result<VerifiedNetworkConfig, NetworkConfigError> {
        if self.config.network != expected_network {
            return Err(NetworkConfigError::WrongNetwork);
        }
        if self.config.configuration_key != trusted_key {
            return Err(NetworkConfigError::UntrustedConfigurationKey);
        }
        if !verifier.verify(trusted_key, &self.config.signing_message(), self.signature) {
            return Err(NetworkConfigError::InvalidSignature);
        }
        Ok(VerifiedNetworkConfig {
            config: self.config,
            protocol_digest: self.config.protocol_digest(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedNetworkConfig {
    config: NetworkConfig,
    protocol_digest: ProtocolDigest,
}

impl VerifiedNetworkConfig {
    #[must_use]
    pub const fn config(self) -> NetworkConfig {
        self.config
    }

    #[must_use]
    pub const fn network(self) -> NetworkId {
        self.config.network
    }

    #[must_use]
    pub const fn genesis_checkpoint(self) -> FinalizedCheckpoint {
        self.config.genesis_checkpoint
    }

    #[must_use]
    pub const fn state_commitment_policy(self) -> StateCommitmentPolicy {
        self.config.state_commitment_policy
    }

    #[must_use]
    pub const fn protocol_digest(self) -> ProtocolDigest {
        self.protocol_digest
    }

    /// Verifies that canonical genesis bytes reproduce the signed checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid ledger bytes or a commitment mismatch.
    pub fn verify_genesis_state(self, canonical_state: &[u8]) -> Result<(), NetworkConfigError> {
        let checkpoint = self.config.genesis_checkpoint;
        let root = self
            .config
            .state_commitment_policy
            .root_for_state(checkpoint.height, self.config.network, canonical_state)
            .map_err(|_| NetworkConfigError::GenesisStateMismatch)?;
        if root == checkpoint.state_root {
            Ok(())
        } else {
            Err(NetworkConfigError::GenesisStateMismatch)
        }
    }
}
