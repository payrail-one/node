use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use network_config_core::{
    ConfigurationPublicKey, ConfigurationSignature, NetworkConfig, NetworkConfigCodec,
    NetworkConfigError, NetworkConfigSignatureVerifier, SignedNetworkConfig,
    SignedNetworkConfigCodec,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const KEY: ConfigurationPublicKey = ConfigurationPublicKey::new([2; 32]);

fn config() -> NetworkConfig {
    NetworkConfig::new(
        NETWORK,
        FinalizedCheckpoint {
            height: 100,
            block_hash: BlockHash::new([3; 32]),
            state_root: StateRoot::new([4; 32]),
            validator_set_hash: ValidatorSetHash::new([5; 32]),
        },
        1,
        KEY,
        StateCommitmentPolicy::authenticated_state_v1_from(101),
    )
    .unwrap()
}

struct ExactVerifier;

impl NetworkConfigSignatureVerifier for ExactVerifier {
    fn verify(
        &self,
        public_key: ConfigurationPublicKey,
        message: &[u8],
        signature: ConfigurationSignature,
    ) -> bool {
        public_key == KEY
            && message == config().signing_message()
            && signature == ConfigurationSignature::new([9; 64])
    }
}

#[test]
fn canonical_and_signed_codecs_round_trip_and_reject_ambiguity() {
    let config = config();
    let encoded = NetworkConfigCodec::encode(config);
    assert_eq!(NetworkConfigCodec::decode(&encoded).unwrap(), config);
    assert_eq!(
        config.protocol_digest().as_bytes(),
        &[
            102, 205, 23, 255, 43, 246, 43, 164, 95, 41, 72, 123, 125, 129, 5, 158, 64, 53, 91, 29,
            143, 42, 158, 187, 122, 21, 18, 94, 186, 220, 84, 196,
        ]
    );

    let signed = SignedNetworkConfig::new(config, ConfigurationSignature::new([9; 64]));
    let encoded_signed = SignedNetworkConfigCodec::encode(signed);
    assert_eq!(
        SignedNetworkConfigCodec::decode(&encoded_signed).unwrap(),
        signed
    );
    assert_eq!(
        SignedNetworkConfigCodec::decode(&encoded_signed[..encoded_signed.len() - 1]),
        Err(NetworkConfigError::InvalidLength)
    );

    let mut non_canonical = encoded;
    non_canonical[188] = 0;
    assert_eq!(
        NetworkConfigCodec::decode(&non_canonical),
        Err(NetworkConfigError::UnsupportedValue)
    );
}

#[test]
fn signature_verification_binds_network_trust_anchor_and_every_config_field() {
    let signed = SignedNetworkConfig::new(config(), ConfigurationSignature::new([9; 64]));
    let verified = signed.verify(NETWORK, KEY, &ExactVerifier).unwrap();
    assert_eq!(verified.network(), NETWORK);
    assert_eq!(verified.protocol_digest(), config().protocol_digest());

    assert_eq!(
        signed.verify(NetworkId::new([8; 32]), KEY, &ExactVerifier),
        Err(NetworkConfigError::WrongNetwork)
    );
    assert_eq!(
        signed.verify(
            NETWORK,
            ConfigurationPublicKey::new([8; 32]),
            &ExactVerifier,
        ),
        Err(NetworkConfigError::UntrustedConfigurationKey)
    );
    assert_eq!(
        SignedNetworkConfig::new(config(), ConfigurationSignature::new([8; 64])).verify(
            NETWORK,
            KEY,
            &ExactVerifier,
        ),
        Err(NetworkConfigError::InvalidSignature)
    );
}

#[test]
fn invalid_trust_anchors_and_activation_before_genesis_are_rejected() {
    let checkpoint = config().genesis_checkpoint();
    assert_eq!(
        NetworkConfig::new(
            NETWORK,
            checkpoint,
            1,
            KEY,
            StateCommitmentPolicy::authenticated_state_v1_from(99),
        ),
        Err(NetworkConfigError::ActivationBeforeGenesis)
    );
    assert_eq!(
        NetworkConfig::new(
            NETWORK,
            checkpoint,
            0,
            KEY,
            StateCommitmentPolicy::default(),
        ),
        Err(NetworkConfigError::ZeroProtocolVersion)
    );
    assert_eq!(
        NetworkConfig::new(
            NetworkId::new([0; 32]),
            checkpoint,
            1,
            KEY,
            StateCommitmentPolicy::default(),
        ),
        Err(NetworkConfigError::ZeroNetwork)
    );
    assert_eq!(
        NetworkConfig::new(
            NETWORK,
            checkpoint,
            1,
            ConfigurationPublicKey::new([0; 32]),
            StateCommitmentPolicy::default(),
        ),
        Err(NetworkConfigError::ZeroConfigurationKey)
    );
    for (checkpoint, expected) in [
        (
            FinalizedCheckpoint {
                block_hash: BlockHash::new([0; 32]),
                ..checkpoint
            },
            NetworkConfigError::ZeroGenesisBlockHash,
        ),
        (
            FinalizedCheckpoint {
                state_root: StateRoot::new([0; 32]),
                ..checkpoint
            },
            NetworkConfigError::ZeroGenesisStateRoot,
        ),
        (
            FinalizedCheckpoint {
                validator_set_hash: ValidatorSetHash::new([0; 32]),
                ..checkpoint
            },
            NetworkConfigError::ZeroValidatorSetHash,
        ),
    ] {
        assert_eq!(
            NetworkConfig::new(
                NETWORK,
                checkpoint,
                1,
                KEY,
                StateCommitmentPolicy::default(),
            ),
            Err(expected)
        );
    }
}
