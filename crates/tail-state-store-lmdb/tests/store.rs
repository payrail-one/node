use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::NetworkId;
use sha2::{Digest, Sha256};
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, ManifestId, StateRoot, SyncCompletion,
    ValidatorSetHash,
};
use tail_state_store_lmdb::{
    CommitOutcome, LmdbStateStoreError, LmdbStoreOptions, LmdbTailStateStore,
};
use tail_sync_core::{
    FinalizedTailBlock, PreparedTailTransition, TailCommitment, TailSyncSession,
    TailTransitionExecutor,
};

const NETWORK: NetworkId = NetworkId::new([41; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([42; 32]);
const PROOF: &[u8] = b"proof";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Finality;

impl FinalityProofVerifier for Finality {
    fn verify(&self, _network: NetworkId, _checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        proof == PROOF
    }
}

struct Transition;

impl TailTransitionExecutor for Transition {
    fn prepare_transition(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
    ) -> Option<PreparedTailTransition> {
        let mut state = previous_state.to_vec();
        state.extend_from_slice(payload);
        Some(PreparedTailTransition {
            commitment: TailCommitment {
                block_hash: BlockHash::new(chained_hash(
                    b"lmdb-test.block\0",
                    previous.block_hash.as_bytes(),
                    payload,
                )),
                state_root: StateRoot::new(digest(b"lmdb-test.state\0", &state)),
            },
            state,
        })
    }
}

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tail-state-lmdb-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn base() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([43; 32]),
        state_root: StateRoot::new([44; 32]),
        validator_set_hash: ValidatorSetHash::new([45; 32]),
    }
}

fn completion() -> SyncCompletion {
    SyncCompletion {
        manifest: ManifestId::new([46; 32]),
        checkpoint: base(),
    }
}

fn candidate(
    network: NetworkId,
    previous: FinalizedCheckpoint,
    previous_state: &[u8],
    payload: &[u8],
) -> FinalizedTailBlock {
    let prepared = Transition
        .prepare_transition(previous, previous_state, payload)
        .unwrap();
    FinalizedTailBlock {
        network,
        parent_hash: previous.block_hash,
        checkpoint: FinalizedCheckpoint {
            height: previous.height + 1,
            block_hash: prepared.commitment.block_hash,
            state_root: prepared.commitment.state_root,
            validator_set_hash: previous.validator_set_hash,
        },
        payload: payload.to_vec(),
        finality_proof: PROOF.to_vec(),
    }
}

fn verified(
    network: NetworkId,
    previous: FinalizedCheckpoint,
    state: &[u8],
    payload: &[u8],
) -> tail_sync_core::VerifiedTailBlock {
    TailSyncSession::new(network, previous)
        .verify_next(
            candidate(network, previous, state, payload),
            state,
            &Finality,
            &Transition,
        )
        .unwrap()
}

#[test]
fn atomic_commit_survives_restart_and_is_idempotent() {
    let path = directory("restart");
    let initial_state = b"snapshot-state";
    let block = verified(NETWORK, base(), initial_state, b"block-101");
    {
        let store = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
        store
            .initialize_from_snapshot(completion(), initial_state)
            .unwrap();
        assert_eq!(
            store.finalized_payload(100),
            Err(LmdbStateStoreError::BlockNotFound)
        );
        assert_eq!(
            store.commit_verified(&block).unwrap(),
            CommitOutcome::Committed
        );
        assert_eq!(
            store.commit_verified(&block).unwrap(),
            CommitOutcome::ExistingSame
        );
        assert_eq!(store.finalized_payload(101).unwrap(), block.payload());
        assert_eq!(store.recovery_base().unwrap().checkpoint, base());
        assert_eq!(store.recovery_base().unwrap().state, initial_state);
        let stored_block = store.finalized_block(101).unwrap();
        assert_eq!(stored_block.previous, base());
        assert_eq!(stored_block.checkpoint, block.checkpoint());
        assert_eq!(stored_block.payload, block.payload());
    }
    let reopened = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
    assert_eq!(reopened.current().unwrap().checkpoint, block.checkpoint());
    assert_eq!(reopened.current().unwrap().state, block.state());
    assert_eq!(reopened.recovery_base().unwrap().state, initial_state);
    assert_eq!(reopened.finalized_payload(101).unwrap(), block.payload());
    assert_eq!(
        reopened.finalized_payload(102),
        Err(LmdbStateStoreError::BlockNotFound)
    );
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn complete_state_retention_is_bounded_to_base_and_current() {
    let path = directory("bounded-state-retention");
    let initial_state = b"snapshot-state";
    let store = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
    store
        .initialize_from_snapshot(completion(), initial_state)
        .unwrap();
    assert_eq!(store.retained_state_count().unwrap(), 1);
    assert_eq!(store.retained_payload_count().unwrap(), 0);

    let first = verified(NETWORK, base(), initial_state, b"block-101");
    store.commit_verified(&first).unwrap();
    assert_eq!(store.retained_state_count().unwrap(), 2);

    let second = verified(NETWORK, first.checkpoint(), first.state(), b"block-102");
    store.commit_verified(&second).unwrap();
    assert_eq!(store.retained_state_count().unwrap(), 2);

    let third = verified(NETWORK, second.checkpoint(), second.state(), b"block-103");
    store.commit_verified(&third).unwrap();
    assert_eq!(store.retained_state_count().unwrap(), 2);
    assert_eq!(store.retained_payload_count().unwrap(), 3);
    assert_eq!(store.current().unwrap().state, third.state());

    drop(store);
    let reopened = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
    assert_eq!(reopened.retained_state_count().unwrap(), 2);
    assert_eq!(reopened.retained_payload_count().unwrap(), 3);
    assert_eq!(reopened.current().unwrap().checkpoint, third.checkpoint());
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn stale_conflict_and_wrong_network_leave_current_state_unchanged() {
    let path = directory("conflict");
    let initial_state = b"snapshot-state";
    let accepted = verified(NETWORK, base(), initial_state, b"accepted");
    let conflict = verified(NETWORK, base(), initial_state, b"conflict");
    let wrong_network = verified(OTHER_NETWORK, base(), initial_state, b"other-network");
    let store = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
    store
        .initialize_from_snapshot(completion(), initial_state)
        .unwrap();
    store.commit_verified(&accepted).unwrap();
    assert_eq!(
        store.commit_verified(&conflict),
        Err(LmdbStateStoreError::CursorMismatch)
    );
    assert_eq!(
        store.commit_verified(&wrong_network),
        Err(LmdbStateStoreError::WrongNetwork)
    );
    let current = store.current().unwrap();
    assert_eq!(current.checkpoint, accepted.checkpoint());
    assert_eq!(current.state, accepted.state());
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn initialization_and_network_binding_fail_closed() {
    let path = directory("network");
    {
        let store = LmdbTailStateStore::open(&path, NETWORK, LmdbStoreOptions::default()).unwrap();
        assert_eq!(store.current(), Err(LmdbStateStoreError::NotInitialized));
        store
            .initialize_from_snapshot(completion(), b"state")
            .unwrap();
        assert_eq!(
            store.current_authenticated_state(),
            Err(LmdbStateStoreError::VerifiedNetworkConfigRequired)
        );
        assert_eq!(
            store.initialize_from_snapshot(completion(), b"state"),
            Err(LmdbStateStoreError::AlreadyInitialized)
        );
    }
    assert_eq!(
        LmdbTailStateStore::open(&path, OTHER_NETWORK, LmdbStoreOptions::default()).unwrap_err(),
        LmdbStateStoreError::WrongNetwork
    );
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn unsafe_small_map_is_rejected_before_open() {
    let path = directory("map-size");
    let options = LmdbStoreOptions {
        map_size: 1024,
        max_readers: 4,
    };
    assert_eq!(
        LmdbTailStateStore::open(&path, NETWORK, options).unwrap_err(),
        LmdbStateStoreError::MapSizeTooSmall
    );
    assert!(!path.exists());
}

#[test]
fn map_full_aborts_state_block_and_cursor_together() {
    let path = directory("map-full-atomicity");
    let initial_state = vec![7_u8; 7 * 1024 * 1024];
    let payload = vec![8_u8; 4 * 1024 * 1024];
    let options = LmdbStoreOptions {
        map_size: 16 * 1024 * 1024,
        max_readers: 4,
    };
    let block = verified(NETWORK, base(), &initial_state, &payload);
    let store = LmdbTailStateStore::open(&path, NETWORK, options).unwrap();
    store
        .initialize_from_snapshot(completion(), &initial_state)
        .unwrap();
    assert_eq!(
        store.commit_verified(&block),
        Err(LmdbStateStoreError::MapFull)
    );
    let current = store.current().unwrap();
    assert_eq!(current.checkpoint, base());
    assert_eq!(current.state, initial_state);
    assert_eq!(
        store.finalized_payload(101),
        Err(LmdbStateStoreError::BlockNotFound)
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

fn chained_hash(domain: &[u8], previous: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(previous);
    hasher.update(payload);
    hasher.finalize().into()
}

fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}
