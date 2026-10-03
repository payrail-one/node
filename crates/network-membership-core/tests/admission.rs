use ledger_core::{AccountId, NetworkId};
use network_membership_core::{
    CertificateFingerprint, Challenge, ConsensusPublicKey, MembershipError, MembershipRegistry,
    NodeCapabilities, NodeId, NodeRecord, PeerHandshake, PeerRole, ProtocolDigest, TransportProof,
    TransportProofVerifier, TransportPublicKey,
};
use sha2::{Digest, Sha256};

const GOVERNANCE: AccountId = AccountId::new([1; 32]);
const UNKNOWN_AUTHORITY: AccountId = AccountId::new([2; 32]);
const NETWORK: NetworkId = NetworkId::new([3; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([4; 32]);
const NODE: NodeId = NodeId::new([5; 32]);
const TRANSPORT_KEY: TransportPublicKey = TransportPublicKey::new([6; 32]);
const CONSENSUS_KEY: ConsensusPublicKey = ConsensusPublicKey::new([7; 32]);
const CERTIFICATE: CertificateFingerprint = CertificateFingerprint::new([11; 32]);
const CHALLENGE: Challenge = Challenge::new([8; 32]);
const PROTOCOL: ProtocolDigest = ProtocolDigest::new([9; 32]);

#[derive(Clone, Copy)]
struct TestProofVerifier;

impl TransportProofVerifier for TestProofVerifier {
    fn verify(
        &self,
        public_key: TransportPublicKey,
        message: &[u8],
        proof: TransportProof,
    ) -> bool {
        proof == test_proof(public_key, message)
    }
}

fn test_proof(key: TransportPublicKey, message: &[u8]) -> TransportProof {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(message);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut proof = [0_u8; 64];
    proof[..32].copy_from_slice(&digest);
    proof[32..].copy_from_slice(&digest);
    TransportProof::new(proof)
}

fn validator_record() -> NodeRecord {
    NodeRecord {
        node: NODE,
        transport_key: TRANSPORT_KEY,
        certificate_fingerprint: CERTIFICATE,
        consensus_key: Some(CONSENSUS_KEY),
        capabilities: NodeCapabilities {
            validator: true,
            sync_provider: true,
            sentry: false,
        },
        active_from: 10,
        revoked_from: None,
        metadata_hash: [10; 32],
    }
}

fn registry() -> MembershipRegistry {
    let mut registry = MembershipRegistry::new(NETWORK, GOVERNANCE);
    registry
        .register_node(GOVERNANCE, validator_record())
        .unwrap();
    registry
}

fn unsigned_handshake(registry: &MembershipRegistry, role: PeerRole) -> PeerHandshake {
    PeerHandshake {
        network: NETWORK,
        node: NODE,
        membership_epoch: registry.epoch(),
        role,
        protocol: PROTOCOL,
        challenge: CHALLENGE,
        proof: TransportProof::default(),
    }
}

fn signed_handshake(registry: &MembershipRegistry, role: PeerRole) -> PeerHandshake {
    let mut handshake = unsigned_handshake(registry, role);
    handshake.proof = test_proof(TRANSPORT_KEY, &handshake.authorization_bytes());
    handshake
}

#[test]
fn registered_validator_with_fresh_proof_is_admitted() {
    let registry = registry();
    let handshake = signed_handshake(&registry, PeerRole::Validator);

    let admission = registry
        .admit(10, PROTOCOL, CHALLENGE, &handshake, &TestProofVerifier)
        .unwrap();

    assert_eq!(admission.network, NETWORK);
    assert_eq!(admission.node, NODE);
    assert_eq!(admission.role, PeerRole::Validator);
    assert_eq!(admission.membership_epoch, 1);
    assert_eq!(admission.protocol, PROTOCOL);
    assert_eq!(admission.challenge, CHALLENGE);
    assert_eq!(admission.transport_key, TRANSPORT_KEY);
    assert_eq!(admission.certificate_fingerprint, CERTIFICATE);
    assert_eq!(admission.consensus_key, Some(CONSENSUS_KEY));
}

#[test]
fn wrong_network_epoch_or_challenge_fails_closed() {
    let registry = registry();

    let mut wrong_network = signed_handshake(&registry, PeerRole::Validator);
    wrong_network.network = OTHER_NETWORK;
    assert_eq!(
        registry.admit(10, PROTOCOL, CHALLENGE, &wrong_network, &TestProofVerifier),
        Err(MembershipError::WrongNetwork)
    );

    let mut stale = signed_handshake(&registry, PeerRole::Validator);
    stale.membership_epoch = 0;
    assert_eq!(
        registry.admit(10, PROTOCOL, CHALLENGE, &stale, &TestProofVerifier),
        Err(MembershipError::StaleMembershipEpoch)
    );

    let handshake = signed_handshake(&registry, PeerRole::Validator);
    assert_eq!(
        registry.admit(
            10,
            PROTOCOL,
            Challenge::new([88; 32]),
            &handshake,
            &TestProofVerifier
        ),
        Err(MembershipError::ChallengeMismatch)
    );
}

#[test]
fn proof_binds_role_protocol_and_fresh_challenge() {
    let registry = registry();
    let signed = signed_handshake(&registry, PeerRole::Validator);

    let mut changed_role = signed;
    changed_role.role = PeerRole::SyncProvider;
    assert_eq!(
        registry.admit(10, PROTOCOL, CHALLENGE, &changed_role, &TestProofVerifier),
        Err(MembershipError::InvalidProof)
    );

    let mut changed_protocol = signed;
    changed_protocol.protocol = ProtocolDigest::new([77; 32]);
    assert_eq!(
        registry.admit(
            10,
            PROTOCOL,
            CHALLENGE,
            &changed_protocol,
            &TestProofVerifier
        ),
        Err(MembershipError::UnsupportedProtocol)
    );

    let mut replayed = signed;
    replayed.challenge = Challenge::new([66; 32]);
    assert_eq!(
        registry.admit(
            10,
            PROTOCOL,
            replayed.challenge,
            &replayed,
            &TestProofVerifier
        ),
        Err(MembershipError::InvalidProof)
    );
}

#[test]
fn unknown_node_and_unregistered_role_are_rejected() {
    let registry = registry();
    let mut unknown = signed_handshake(&registry, PeerRole::Validator);
    unknown.node = NodeId::new([44; 32]);
    assert_eq!(
        registry.admit(10, PROTOCOL, CHALLENGE, &unknown, &TestProofVerifier),
        Err(MembershipError::NodeNotFound)
    );

    let sentry = signed_handshake(&registry, PeerRole::Sentry);
    assert_eq!(
        registry.admit(10, PROTOCOL, CHALLENGE, &sentry, &TestProofVerifier),
        Err(MembershipError::RoleNotAllowed)
    );
}

#[test]
fn activation_and_finalized_height_revocation_are_enforced() {
    let mut registry = registry();
    let before_activation = signed_handshake(&registry, PeerRole::Validator);
    assert_eq!(
        registry.admit(
            9,
            PROTOCOL,
            CHALLENGE,
            &before_activation,
            &TestProofVerifier
        ),
        Err(MembershipError::NotYetActive)
    );

    assert_eq!(registry.revoke_node(GOVERNANCE, NODE, 20), Ok(2));
    let current = signed_handshake(&registry, PeerRole::Validator);
    assert!(
        registry
            .admit(19, PROTOCOL, CHALLENGE, &current, &TestProofVerifier)
            .is_ok()
    );
    assert_eq!(
        registry.admit(20, PROTOCOL, CHALLENGE, &current, &TestProofVerifier),
        Err(MembershipError::Revoked)
    );
}

#[test]
fn every_membership_change_invalidates_stale_handshakes() {
    let mut registry = registry();
    let old = signed_handshake(&registry, PeerRole::Validator);
    registry.revoke_node(GOVERNANCE, NODE, 20).unwrap();

    assert_eq!(
        registry.admit(15, PROTOCOL, CHALLENGE, &old, &TestProofVerifier),
        Err(MembershipError::StaleMembershipEpoch)
    );
}

#[test]
fn registration_requires_governance_and_valid_role_key_shape() {
    let mut registry = MembershipRegistry::new(NETWORK, GOVERNANCE);
    assert_eq!(
        registry.register_node(UNKNOWN_AUTHORITY, validator_record()),
        Err(MembershipError::Unauthorized)
    );

    let mut empty = validator_record();
    empty.capabilities = NodeCapabilities::default();
    assert_eq!(
        registry.register_node(GOVERNANCE, empty),
        Err(MembershipError::InvalidCapabilities)
    );

    let mut validator_without_key = validator_record();
    validator_without_key.consensus_key = None;
    assert_eq!(
        registry.register_node(GOVERNANCE, validator_without_key),
        Err(MembershipError::InvalidConsensusKey)
    );

    let mut non_validator_with_key = validator_record();
    non_validator_with_key.capabilities.validator = false;
    assert_eq!(
        registry.register_node(GOVERNANCE, non_validator_with_key),
        Err(MembershipError::InvalidConsensusKey)
    );
}

#[test]
fn node_transport_and_consensus_keys_are_unique() {
    let mut registry = registry();
    assert_eq!(
        registry.register_node(GOVERNANCE, validator_record()),
        Err(MembershipError::NodeAlreadyRegistered)
    );

    let mut duplicate_transport = validator_record();
    duplicate_transport.node = NodeId::new([20; 32]);
    duplicate_transport.consensus_key = Some(ConsensusPublicKey::new([21; 32]));
    assert_eq!(
        registry.register_node(GOVERNANCE, duplicate_transport),
        Err(MembershipError::TransportKeyAlreadyRegistered)
    );

    let mut duplicate_consensus = validator_record();
    duplicate_consensus.node = NodeId::new([22; 32]);
    duplicate_consensus.transport_key = TransportPublicKey::new([23; 32]);
    duplicate_consensus.certificate_fingerprint = CertificateFingerprint::new([24; 32]);
    assert_eq!(
        registry.register_node(GOVERNANCE, duplicate_consensus),
        Err(MembershipError::ConsensusKeyAlreadyRegistered)
    );

    let mut duplicate_certificate = validator_record();
    duplicate_certificate.node = NodeId::new([25; 32]);
    duplicate_certificate.transport_key = TransportPublicKey::new([26; 32]);
    duplicate_certificate.consensus_key = Some(ConsensusPublicKey::new([27; 32]));
    assert_eq!(
        registry.register_node(GOVERNANCE, duplicate_certificate),
        Err(MembershipError::CertificateAlreadyRegistered)
    );
}
