use finality_proof_core::{
    ConsensusSignature, FinalityCertificate, FinalityCertificateCodec, FinalityError,
    FinalitySignatureVerifier, FinalityVerifier, Validator, ValidatorSet, ValidatorSignature,
};
use ledger_core::NetworkId;
use network_membership_core::{ConsensusPublicKey, NodeId};
use sha2::{Digest, Sha256};
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, StateRoot, ValidatorSetHash,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);

#[derive(Clone, Copy)]
struct TestVerifier;

impl FinalitySignatureVerifier for TestVerifier {
    fn verify(
        &self,
        public_key: ConsensusPublicKey,
        message: &[u8],
        signature: ConsensusSignature,
    ) -> bool {
        signature == test_signature(public_key, message)
    }
}

fn test_signature(key: ConsensusPublicKey, message: &[u8]) -> ConsensusSignature {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(message);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&digest);
    bytes[32..].copy_from_slice(&digest);
    ConsensusSignature::new(bytes)
}

fn validators(count: u8) -> Vec<Validator> {
    (1..=count)
        .map(|value| Validator {
            node: NodeId::new([value; 32]),
            consensus_key: ConsensusPublicKey::new([value.wrapping_add(20); 32]),
            weight: 1,
        })
        .collect()
}

fn validator_set() -> ValidatorSet {
    ValidatorSet::new(NETWORK, 11, validators(7)).unwrap()
}

fn checkpoint(set: &ValidatorSet) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 42,
        block_hash: BlockHash::new([2; 32]),
        state_root: StateRoot::new([3; 32]),
        validator_set_hash: set.hash(),
    }
}

fn certificate(set: &ValidatorSet, signer_count: u8) -> FinalityCertificate {
    let mut certificate = FinalityCertificate {
        network: NETWORK,
        membership_epoch: 11,
        round: 4,
        checkpoint: checkpoint(set),
        signatures: Vec::new(),
    };
    let message = certificate.signing_message();
    certificate.signatures = validators(signer_count)
        .into_iter()
        .map(|validator| ValidatorSignature {
            node: validator.node,
            signature: test_signature(validator.consensus_key, &message),
        })
        .collect();
    certificate
}

#[test]
fn five_of_seven_equal_weight_validators_form_quorum() {
    let set = validator_set();
    assert_eq!(set.total_weight(), 7);
    assert_eq!(set.quorum_weight(), 5);
    let proof = certificate(&set, 5);
    let verified = FinalityVerifier::new(set.clone(), TestVerifier)
        .verify_certificate(NETWORK, checkpoint(&set), &proof)
        .unwrap();

    assert_eq!(verified.validator_set, set.hash());
    assert_eq!(verified.signed_weight, 5);
    assert_eq!(verified.quorum_weight, 5);
}

#[test]
fn two_offline_validators_preserve_liveness_but_three_do_not() {
    let set = validator_set();
    let verifier = FinalityVerifier::new(set.clone(), TestVerifier);

    assert!(
        verifier
            .verify_certificate(NETWORK, checkpoint(&set), &certificate(&set, 5))
            .is_ok()
    );
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &certificate(&set, 4)),
        Err(FinalityError::InsufficientQuorum)
    );
}

#[test]
fn proof_codec_is_canonical_bounded_and_connects_to_state_sync_trait() {
    let set = validator_set();
    let certificate = certificate(&set, 5);
    let encoded = FinalityCertificateCodec::encode(&certificate).unwrap();

    assert_eq!(
        FinalityCertificateCodec::decode(&encoded),
        Ok(certificate.clone())
    );
    for length in 0..encoded.len() {
        assert!(FinalityCertificateCodec::decode(&encoded[..length]).is_err());
    }
    let verifier = FinalityVerifier::new(set.clone(), TestVerifier);
    assert!(verifier.verify(NETWORK, checkpoint(&set), &encoded));
    let mut changed = checkpoint(&set);
    changed.state_root = StateRoot::new([99; 32]);
    assert!(!verifier.verify(NETWORK, changed, &encoded));

    let mut trailing = encoded;
    trailing.push(0);
    assert_eq!(
        FinalityCertificateCodec::decode(&trailing),
        Err(FinalityError::TrailingBytes)
    );
}

#[test]
fn invalid_unknown_duplicate_or_reordered_signatures_fail_closed() {
    let set = validator_set();
    let verifier = FinalityVerifier::new(set.clone(), TestVerifier);
    let mut invalid = certificate(&set, 5);
    invalid.signatures[0].signature = ConsensusSignature::new([0; 64]);
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &invalid),
        Err(FinalityError::InvalidSignature)
    );

    let mut unknown = certificate(&set, 5);
    unknown.signatures[4].node = NodeId::new([100; 32]);
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &unknown),
        Err(FinalityError::UnknownValidator)
    );

    let mut duplicate = certificate(&set, 5);
    duplicate.signatures[1].node = duplicate.signatures[0].node;
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &duplicate),
        Err(FinalityError::NonCanonicalSignatureOrder)
    );

    let mut reordered = certificate(&set, 5);
    reordered.signatures.swap(0, 1);
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &reordered),
        Err(FinalityError::NonCanonicalSignatureOrder)
    );
}

#[test]
fn validator_set_rejects_ambiguous_or_unsafe_configuration() {
    assert_eq!(
        ValidatorSet::new(NETWORK, 1, Vec::new()),
        Err(FinalityError::EmptyValidatorSet)
    );
    let mut unordered = validators(2);
    unordered.swap(0, 1);
    assert_eq!(
        ValidatorSet::new(NETWORK, 1, unordered),
        Err(FinalityError::NonCanonicalValidatorOrder)
    );
    let mut duplicate_key = validators(2);
    duplicate_key[1].consensus_key = duplicate_key[0].consensus_key;
    assert_eq!(
        ValidatorSet::new(NETWORK, 1, duplicate_key),
        Err(FinalityError::DuplicateConsensusKey)
    );
    let mut zero_weight = validators(1);
    zero_weight[0].weight = 0;
    assert_eq!(
        ValidatorSet::new(NETWORK, 1, zero_weight),
        Err(FinalityError::ZeroValidatorWeight)
    );
    let overflow = vec![
        Validator {
            node: NodeId::new([1; 32]),
            consensus_key: ConsensusPublicKey::new([1; 32]),
            weight: u64::MAX,
        },
        Validator {
            node: NodeId::new([2; 32]),
            consensus_key: ConsensusPublicKey::new([2; 32]),
            weight: 1,
        },
    ];
    assert_eq!(
        ValidatorSet::new(NETWORK, 1, overflow),
        Err(FinalityError::TotalWeightOverflow)
    );
}

#[test]
fn wrong_network_epoch_set_or_checkpoint_is_rejected() {
    let set = validator_set();
    let verifier = FinalityVerifier::new(set.clone(), TestVerifier);
    let valid = certificate(&set, 5);

    assert_eq!(
        verifier.verify_certificate(NetworkId::new([9; 32]), checkpoint(&set), &valid),
        Err(FinalityError::WrongNetwork)
    );
    let mut wrong_epoch = valid.clone();
    wrong_epoch.membership_epoch = 12;
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &wrong_epoch),
        Err(FinalityError::MembershipEpochMismatch)
    );
    let mut wrong_set = valid.clone();
    wrong_set.checkpoint.validator_set_hash = ValidatorSetHash::new([8; 32]);
    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &wrong_set),
        Err(FinalityError::ValidatorSetMismatch)
    );
    let mut expected = checkpoint(&set);
    expected.height += 1;
    assert_eq!(
        verifier.verify_certificate(NETWORK, expected, &valid),
        Err(FinalityError::CheckpointMismatch)
    );
}
