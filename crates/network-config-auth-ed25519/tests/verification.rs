use ed25519_dalek::{Signer, SigningKey};
use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use network_config_auth_ed25519::Ed25519NetworkConfigVerifier;
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, NetworkConfigError,
    SignedNetworkConfig,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

#[test]
fn strict_ed25519_verification_accepts_only_the_pinned_configuration_key() {
    let network = NetworkId::new([11; 32]);
    let key = SigningKey::from_bytes(&[12; 32]);
    let public_key = ConfigurationPublicKey::new(key.verifying_key().to_bytes());
    let config = NetworkConfig::new(
        network,
        FinalizedCheckpoint {
            height: 10,
            block_hash: BlockHash::new([13; 32]),
            state_root: StateRoot::new([14; 32]),
            validator_set_hash: ValidatorSetHash::new([15; 32]),
        },
        1,
        public_key,
        StateCommitmentPolicy::authenticated_state_v1_from(11),
    )
    .unwrap();
    let signed = SignedNetworkConfig::new(
        config,
        ConfigurationSignature::new(key.sign(&config.signing_message()).to_bytes()),
    );

    assert!(
        signed
            .verify(network, public_key, &Ed25519NetworkConfigVerifier)
            .is_ok()
    );
    let mut forged = signed.signature().as_bytes().to_owned();
    forged[0] ^= 1;
    assert_eq!(
        SignedNetworkConfig::new(config, ConfigurationSignature::new(forged)).verify(
            network,
            public_key,
            &Ed25519NetworkConfigVerifier,
        ),
        Err(NetworkConfigError::InvalidSignature)
    );
}
