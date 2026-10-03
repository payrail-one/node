use std::collections::BTreeMap;

use ledger_core::NetworkId;
use network_membership_core::{
    Admission, CertificateFingerprint, Challenge, ConsensusPublicKey, NodeId, PeerRole,
    ProtocolDigest, TransportPublicKey,
};
use state_sync_core::{
    BlockHash, ChunkAcceptance, ChunkDescriptor, ChunkHash, FinalityProofVerifier,
    FinalizedCheckpoint, MAX_CHUNK_BYTES, MAX_FINALITY_PROOF_BYTES, ManifestId, SnapshotFormatId,
    SnapshotManifest, StateRoot, StateSyncError, StateSyncSession, SyncProgress, ValidatorSetHash,
    VerifiedChunkReader, VerifiedChunkStore,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([2; 32]);
const PROTOCOL: ProtocolDigest = ProtocolDigest::new([3; 32]);
const OTHER_PROTOCOL: ProtocolDigest = ProtocolDigest::new([4; 32]);
const FORMAT: SnapshotFormatId = SnapshotFormatId::new([5; 32]);
const STATE_ROOT: StateRoot = StateRoot::new([6; 32]);
const PROOF: &[u8] = b"verified-finality-proof";

#[derive(Default)]
struct MemoryStore {
    writes: Vec<(ManifestId, u32, Vec<u8>)>,
    fail: bool,
}

impl VerifiedChunkStore for MemoryStore {
    type Error = ();

    fn persist_verified(
        &mut self,
        manifest: ManifestId,
        descriptor: ChunkDescriptor,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        if self.fail {
            return Err(());
        }
        self.writes
            .push((manifest, descriptor.index, bytes.to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct RecoveryReader {
    chunks: BTreeMap<u32, Vec<u8>>,
    fail: bool,
}

impl VerifiedChunkReader for RecoveryReader {
    type Error = ();

    fn load_verified(
        &self,
        _manifest: ManifestId,
        descriptor: ChunkDescriptor,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        if self.fail {
            return Err(());
        }
        Ok(self.chunks.get(&descriptor.index).cloned())
    }
}

#[derive(Clone, Copy)]
struct TestFinalityVerifier;

impl FinalityProofVerifier for TestFinalityVerifier {
    fn verify(&self, network: NetworkId, checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        network == NETWORK && checkpoint.height == 42 && proof == PROOF
    }
}

fn checkpoint() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 42,
        block_hash: BlockHash::new([7; 32]),
        state_root: STATE_ROOT,
        validator_set_hash: ValidatorSetHash::new([8; 32]),
    }
}

fn manifest() -> SnapshotManifest {
    let first = b"canonical-state-";
    let second = b"snapshot";
    SnapshotManifest {
        network: NETWORK,
        protocol: PROTOCOL,
        snapshot_format: FORMAT,
        checkpoint: checkpoint(),
        total_bytes: u64::try_from(first.len() + second.len()).unwrap(),
        chunks: vec![
            ChunkDescriptor {
                index: 0,
                offset: 0,
                length: u32::try_from(first.len()).unwrap(),
                hash: ChunkHash::digest(first),
            },
            ChunkDescriptor {
                index: 1,
                offset: u64::try_from(first.len()).unwrap(),
                length: u32::try_from(second.len()).unwrap(),
                hash: ChunkHash::digest(second),
            },
        ],
    }
}

fn provider(network: NetworkId, epoch: u64, role: PeerRole) -> Admission {
    Admission {
        network,
        node: NodeId::new([9; 32]),
        role,
        membership_epoch: epoch,
        protocol: PROTOCOL,
        challenge: Challenge::new([12; 32]),
        transport_key: TransportPublicKey::new([10; 32]),
        certificate_fingerprint: CertificateFingerprint::new([13; 32]),
        consensus_key: Some(ConsensusPublicKey::new([11; 32])),
    }
}

fn session() -> StateSyncSession {
    StateSyncSession::start(
        NETWORK,
        PROTOCOL,
        7,
        manifest(),
        PROOF,
        &TestFinalityVerifier,
    )
    .unwrap()
}

#[test]
fn verified_chunks_can_arrive_out_of_order_and_retries_are_idempotent() {
    let mut session = session();
    let mut store = MemoryStore::default();
    let sync_provider = provider(NETWORK, 7, PeerRole::SyncProvider);

    assert_eq!(
        session.accept_chunk(sync_provider, 1, b"snapshot", &mut store),
        Ok(ChunkAcceptance::Added)
    );
    assert_eq!(
        session.accept_chunk(sync_provider, 1, b"snapshot", &mut store),
        Ok(ChunkAcceptance::Duplicate)
    );
    assert_eq!(
        session.progress(),
        SyncProgress {
            received_chunks: 1,
            total_chunks: 2,
            received_bytes: 8,
            total_bytes: 24,
        }
    );
    assert_eq!(
        session.accept_chunk(sync_provider, 0, b"canonical-state-", &mut store),
        Ok(ChunkAcceptance::Added)
    );

    let completion = session.finish(STATE_ROOT).unwrap();
    assert_eq!(completion.manifest, session.manifest_id());
    assert_eq!(completion.checkpoint, checkpoint());
    assert_eq!(store.writes.len(), 2);
}

#[test]
fn session_requires_expected_network_protocol_and_finality_proof() {
    assert_eq!(
        StateSyncSession::start(
            OTHER_NETWORK,
            PROTOCOL,
            7,
            manifest(),
            PROOF,
            &TestFinalityVerifier
        )
        .unwrap_err(),
        StateSyncError::WrongNetwork
    );
    assert_eq!(
        StateSyncSession::start(
            NETWORK,
            OTHER_PROTOCOL,
            7,
            manifest(),
            PROOF,
            &TestFinalityVerifier
        )
        .unwrap_err(),
        StateSyncError::UnsupportedProtocol
    );
    assert_eq!(
        StateSyncSession::start(
            NETWORK,
            PROTOCOL,
            7,
            manifest(),
            b"forged",
            &TestFinalityVerifier
        )
        .unwrap_err(),
        StateSyncError::InvalidFinalityProof
    );
}

#[test]
fn oversized_finality_proof_is_rejected_before_verification() {
    let proof = vec![0_u8; MAX_FINALITY_PROOF_BYTES + 1];

    assert_eq!(
        StateSyncSession::start(
            NETWORK,
            PROTOCOL,
            7,
            manifest(),
            &proof,
            &TestFinalityVerifier
        )
        .unwrap_err(),
        StateSyncError::FinalityProofTooLarge
    );
}

#[test]
fn only_current_admitted_sync_providers_can_supply_chunks() {
    let mut session = session();
    let mut store = MemoryStore::default();

    assert_eq!(
        session.accept_chunk(
            provider(OTHER_NETWORK, 7, PeerRole::SyncProvider),
            0,
            b"canonical-state-",
            &mut store
        ),
        Err(StateSyncError::ProviderWrongNetwork)
    );
    assert_eq!(
        session.accept_chunk(
            provider(NETWORK, 6, PeerRole::SyncProvider),
            0,
            b"canonical-state-",
            &mut store
        ),
        Err(StateSyncError::ProviderEpochMismatch)
    );
    assert_eq!(
        session.accept_chunk(
            provider(NETWORK, 7, PeerRole::Validator),
            0,
            b"canonical-state-",
            &mut store
        ),
        Err(StateSyncError::ProviderRoleNotAllowed)
    );
}

#[test]
fn chunk_length_hash_and_index_are_verified() {
    let mut session = session();
    let mut store = MemoryStore::default();
    let sync_provider = provider(NETWORK, 7, PeerRole::SyncProvider);

    assert_eq!(
        session.accept_chunk(sync_provider, 9, b"unknown", &mut store),
        Err(StateSyncError::UnknownChunk)
    );
    assert_eq!(
        session.accept_chunk(sync_provider, 0, b"short", &mut store),
        Err(StateSyncError::ChunkLengthMismatch)
    );
    assert_eq!(
        session.accept_chunk(sync_provider, 0, b"tampered-state--", &mut store),
        Err(StateSyncError::ChunkHashMismatch)
    );
    assert_eq!(session.progress().received_chunks, 0);
}

#[test]
fn storage_failure_does_not_advance_sync_progress() {
    let mut session = session();
    let mut store = MemoryStore {
        fail: true,
        ..MemoryStore::default()
    };

    assert_eq!(
        session.accept_chunk(
            provider(NETWORK, 7, PeerRole::SyncProvider),
            0,
            b"canonical-state-",
            &mut store,
        ),
        Err(StateSyncError::StorageFailure)
    );
    assert_eq!(session.progress().received_chunks, 0);
    assert_eq!(session.progress().received_bytes, 0);
}

#[test]
fn persisted_progress_is_recovered_idempotently_after_restart() {
    let mut restarted = session();
    let reader = RecoveryReader {
        chunks: BTreeMap::from([(0, b"canonical-state-".to_vec())]),
        fail: false,
    };
    let expected = SyncProgress {
        received_chunks: 1,
        total_chunks: 2,
        received_bytes: 16,
        total_bytes: 24,
    };
    assert_eq!(restarted.recover_persisted(&reader), Ok(expected));
    assert_eq!(restarted.recover_persisted(&reader), Ok(expected));
}

#[test]
fn corrupt_or_unreadable_persisted_progress_fails_closed() {
    let corrupt = RecoveryReader {
        chunks: BTreeMap::from([(0, b"tampered-state--".to_vec())]),
        fail: false,
    };
    let mut corrupt_session = session();
    assert_eq!(
        corrupt_session.recover_persisted(&corrupt),
        Err(StateSyncError::StoredChunkCorrupt)
    );
    assert_eq!(corrupt_session.progress().received_chunks, 0);

    let mut failed_session = session();
    assert_eq!(
        failed_session.recover_persisted(&RecoveryReader {
            chunks: BTreeMap::new(),
            fail: true,
        }),
        Err(StateSyncError::StorageFailure)
    );
    assert_eq!(failed_session.progress().received_chunks, 0);
}

#[test]
fn malformed_manifest_layouts_are_rejected() {
    let mut empty = manifest();
    empty.chunks.clear();
    empty.total_bytes = 0;
    assert_eq!(
        empty.manifest_id().unwrap_err(),
        StateSyncError::EmptyManifest
    );

    let mut gap = manifest();
    gap.chunks[1].offset += 1;
    assert_eq!(
        gap.manifest_id().unwrap_err(),
        StateSyncError::NonCanonicalChunkLayout
    );

    let mut wrong_total = manifest();
    wrong_total.total_bytes += 1;
    assert_eq!(
        wrong_total.manifest_id().unwrap_err(),
        StateSyncError::TotalSizeMismatch
    );

    let mut oversized = manifest();
    oversized.chunks[0].length = u32::try_from(MAX_CHUNK_BYTES + 1).unwrap();
    oversized.chunks[1].offset = u64::from(oversized.chunks[0].length);
    oversized.total_bytes = oversized.chunks[1].offset + u64::from(oversized.chunks[1].length);
    assert_eq!(
        oversized.manifest_id().unwrap_err(),
        StateSyncError::ChunkTooLarge
    );
}

#[test]
fn completion_requires_all_chunks_and_exact_applied_state_root() {
    let mut session = session();
    let mut store = MemoryStore::default();
    let sync_provider = provider(NETWORK, 7, PeerRole::SyncProvider);
    session
        .accept_chunk(sync_provider, 0, b"canonical-state-", &mut store)
        .unwrap();
    assert_eq!(
        session.finish(STATE_ROOT),
        Err(StateSyncError::SyncIncomplete)
    );
    session
        .accept_chunk(sync_provider, 1, b"snapshot", &mut store)
        .unwrap();
    assert_eq!(
        session.finish(StateRoot::new([99; 32])),
        Err(StateSyncError::StateRootMismatch)
    );
}

#[test]
fn manifest_identifier_has_a_stable_independent_vector() {
    assert_eq!(
        manifest().manifest_id(),
        Ok(ManifestId::new([
            143, 229, 49, 15, 115, 186, 30, 12, 124, 25, 160, 97, 1, 14, 62, 1, 109, 239, 76, 199,
            92, 193, 97, 197, 14, 198, 22, 244, 223, 153, 200, 116,
        ]))
    );
}
