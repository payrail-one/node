use ed25519_dalek::{Signer, SigningKey};
use finality_auth_ed25519::Ed25519FinalityVerifier;
use finality_proof_core::{
    ConsensusSignature, FinalityCertificate, FinalityError, FinalityVerifier, Validator,
    ValidatorSet, ValidatorSignature,
};
use ledger_core::NetworkId;
use network_membership_core::{ConsensusPublicKey, NodeId};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot};

const NETWORK: NetworkId = NetworkId::new([1; 32]);

fn signing_keys() -> Vec<SigningKey> {
    (1_u8..=7)
        .map(|value| SigningKey::from_bytes(&[value; 32]))
        .collect()
}

fn validator_set(keys: &[SigningKey]) -> ValidatorSet {
    let validators = keys
        .iter()
        .enumerate()
        .map(|(index, key)| Validator {
            node: NodeId::new([u8::try_from(index + 1).unwrap(); 32]),
            consensus_key: ConsensusPublicKey::new(key.verifying_key().to_bytes()),
            weight: 1,
        })
        .collect();
    ValidatorSet::new(NETWORK, 3, validators).unwrap()
}

fn checkpoint(set: &ValidatorSet) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 900,
        block_hash: BlockHash::new([9; 32]),
        state_root: StateRoot::new([10; 32]),
        validator_set_hash: set.hash(),
    }
}

fn signed_certificate(
    set: &ValidatorSet,
    keys: &[SigningKey],
    online: &[usize],
) -> FinalityCertificate {
    let mut certificate = FinalityCertificate {
        network: NETWORK,
        membership_epoch: 3,
        round: 12,
        checkpoint: checkpoint(set),
        signatures: Vec::new(),
    };
    let message = certificate.signing_message();
    certificate.signatures = online
        .iter()
        .map(|index| ValidatorSignature {
            node: NodeId::new([u8::try_from(index + 1).unwrap(); 32]),
            signature: ConsensusSignature::new(keys[*index].sign(&message).to_bytes()),
        })
        .collect();
    certificate
}

#[test]
fn seven_validators_across_three_domains_tolerate_two_offline_nodes() {
    let keys = signing_keys();
    let set = validator_set(&keys);
    let verifier = FinalityVerifier::new(set.clone(), Ed25519FinalityVerifier);
    let failure_domains: &[&[usize]] = &[&[0, 1, 2], &[3, 4], &[5, 6]];

    let one_offline = [0_usize, 1, 2, 3, 4, 5];
    assert!(
        verifier
            .verify_certificate(
                NETWORK,
                checkpoint(&set),
                &signed_certificate(&set, &keys, &one_offline),
            )
            .is_ok()
    );

    let domain_offline = failure_domains[2];
    let two_offline: Vec<_> = (0..7)
        .filter(|index| !domain_offline.contains(index))
        .collect();
    assert!(
        verifier
            .verify_certificate(
                NETWORK,
                checkpoint(&set),
                &signed_certificate(&set, &keys, &two_offline),
            )
            .is_ok()
    );

    let three_offline = [0_usize, 1, 3, 4];
    assert_eq!(
        verifier.verify_certificate(
            NETWORK,
            checkpoint(&set),
            &signed_certificate(&set, &keys, &three_offline),
        ),
        Err(FinalityError::InsufficientQuorum)
    );
}

#[test]
fn one_forged_ed25519_vote_invalidates_the_certificate() {
    let keys = signing_keys();
    let set = validator_set(&keys);
    let verifier = FinalityVerifier::new(set.clone(), Ed25519FinalityVerifier);
    let mut certificate = signed_certificate(&set, &keys, &[0, 1, 2, 3, 4]);
    certificate.signatures[2].signature = ConsensusSignature::new([0; 64]);

    assert_eq!(
        verifier.verify_certificate(NETWORK, checkpoint(&set), &certificate),
        Err(FinalityError::InvalidSignature)
    );
}
