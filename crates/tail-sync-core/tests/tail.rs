use ledger_core::NetworkId;
use sha2::{Digest, Sha256};
use state_sync_core::{
    BlockHash, FinalityProofVerifier, FinalizedCheckpoint, StateRoot, ValidatorSetHash,
};
use tail_sync_core::{
    FinalizedTailBlock, PreparedTailTransition, TailBlockCodec, TailCommitment, TailSyncError,
    TailSyncSession, TailTransitionExecutor,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const PROOF: &[u8] = b"verified";

struct Finality;

impl FinalityProofVerifier for Finality {
    fn verify(&self, network: NetworkId, checkpoint: FinalizedCheckpoint, proof: &[u8]) -> bool {
        network == NETWORK
            && checkpoint.validator_set_hash == ValidatorSetHash::new([4; 32])
            && proof == PROOF
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
            commitment: commitment(previous, payload, &state),
            state,
        })
    }
}

fn genesis() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 10,
        block_hash: BlockHash::new([2; 32]),
        state_root: StateRoot::new([3; 32]),
        validator_set_hash: ValidatorSetHash::new([4; 32]),
    }
}

fn block(
    previous: FinalizedCheckpoint,
    previous_state: &[u8],
    payload: &[u8],
) -> FinalizedTailBlock {
    let mut state = previous_state.to_vec();
    state.extend_from_slice(payload);
    let commitment = commitment(previous, payload, &state);
    FinalizedTailBlock {
        network: NETWORK,
        parent_hash: previous.block_hash,
        checkpoint: FinalizedCheckpoint {
            height: previous.height + 1,
            block_hash: commitment.block_hash,
            state_root: commitment.state_root,
            validator_set_hash: previous.validator_set_hash,
        },
        payload: payload.to_vec(),
        finality_proof: PROOF.to_vec(),
    }
}

fn commitment(previous: FinalizedCheckpoint, payload: &[u8], state: &[u8]) -> TailCommitment {
    TailCommitment {
        block_hash: BlockHash::new(hash(
            b"tail-test.block\0",
            previous.block_hash.as_bytes(),
            payload,
        )),
        state_root: StateRoot::new(digest(b"tail-test.state\0", state)),
    }
}

fn digest(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    hasher.finalize().into()
}

fn hash(domain: &[u8], previous: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(previous);
    hasher.update(payload);
    hasher.finalize().into()
}

#[test]
fn sequential_verified_blocks_advance_only_after_commit() {
    let mut session = TailSyncSession::new(NETWORK, genesis());
    let mut state = b"genesis".to_vec();
    for payload in [b"block-11".as_slice(), b"block-12".as_slice()] {
        let candidate = block(session.current(), &state, payload);
        let verified = session
            .verify_next(candidate, &state, &Finality, &Transition)
            .unwrap();
        assert_eq!(session.current().height + 1, verified.checkpoint().height);
        state = verified.state().to_vec();
        session.commit(&verified).unwrap();
    }
    assert_eq!(session.current().height, 12);
}

#[test]
fn auxiliary_preparation_runs_only_after_finality_and_is_returned() {
    let state = b"genesis";
    let session = TailSyncSession::new(NETWORK, genesis());
    let candidate = block(session.current(), state, b"prepared");

    let (verified, marker) = session
        .verify_next_with(candidate, &Finality, |previous, payload| {
            Transition
                .prepare_transition(previous, state, payload)
                .map(|prepared| (prepared, 42_u8))
        })
        .unwrap();

    assert_eq!(verified.previous(), session.current());
    assert_eq!(verified.state(), b"genesisprepared");
    assert_eq!(marker, 42);

    let mut forged = block(session.current(), state, b"prepared");
    forged.finality_proof = b"forged".to_vec();
    let mut called = false;
    assert_eq!(
        session.verify_next_with(forged, &Finality, |_, _| {
            called = true;
            None::<(PreparedTailTransition, ())>
        }),
        Err(TailSyncError::InvalidFinalityProof)
    );
    assert!(!called);
}

#[test]
fn gaps_forks_and_invalid_proofs_never_advance_cursor() {
    let session = TailSyncSession::new(NETWORK, genesis());
    let state = b"genesis";
    let mut gap = block(genesis(), state, b"gap");
    gap.checkpoint.height += 1;
    assert_eq!(
        session.verify_next(gap, state, &Finality, &Transition),
        Err(TailSyncError::NonSequentialHeight)
    );
    let mut fork = block(genesis(), state, b"fork");
    fork.parent_hash = BlockHash::new([9; 32]);
    assert_eq!(
        session.verify_next(fork, state, &Finality, &Transition),
        Err(TailSyncError::ParentHashMismatch)
    );
    let mut forged = block(genesis(), state, b"forged");
    forged.finality_proof = b"forged".to_vec();
    assert_eq!(
        session.verify_next(forged, state, &Finality, &Transition),
        Err(TailSyncError::InvalidFinalityProof)
    );
    assert_eq!(session.current(), genesis());
}

#[test]
fn transition_commitments_and_stale_verified_blocks_fail_closed() {
    let mut session = TailSyncSession::new(NETWORK, genesis());
    let state = b"genesis";
    let mut mismatched = block(genesis(), state, b"bad-root");
    mismatched.checkpoint.state_root = StateRoot::new([8; 32]);
    assert_eq!(
        session.verify_next(mismatched, state, &Finality, &Transition),
        Err(TailSyncError::InvalidTransition)
    );

    let first = session
        .verify_next(
            block(genesis(), state, b"first"),
            state,
            &Finality,
            &Transition,
        )
        .unwrap();
    let stale_block = first.clone();
    session.commit(&first).unwrap();
    assert_eq!(
        session.commit(&stale_block),
        Err(TailSyncError::StaleVerifiedBlock)
    );
}

#[test]
fn canonical_codec_round_trips_and_rejects_trailing_data() {
    let candidate = block(genesis(), b"genesis", b"encoded");
    let encoded = TailBlockCodec::encode(&candidate).unwrap();
    assert_eq!(TailBlockCodec::decode(&encoded), Ok(candidate));
    let mut trailing = encoded;
    trailing.push(0);
    assert_eq!(
        TailBlockCodec::decode(&trailing),
        Err(TailSyncError::TrailingBytes)
    );
}
