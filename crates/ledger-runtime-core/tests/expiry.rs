mod support;

use ed25519_dalek::SigningKey;
use ledger_core::{AccountId, Ledger, OperationOutcome};
use ledger_runtime_core::{
    LedgerBlockExecutor, LedgerStateCodec, ProposalLimits, RuntimeError, state_root,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, ValidatorSetHash};
use transaction_auth_ed25519::Ed25519Verifier;

use support::*;

#[test]
fn expired_operation_consumes_nonce_without_moving_value_and_unblocks_next_payment() {
    let key = sender_key();
    let (sender, previous_state, previous) = initial_state();
    let expired = signed_payment_until(&key, RECIPIENT, 0, 13, 900, 9, previous.height);
    let applied = signed_payment_until(&key, RECIPIENT, 1, 14, 100, 1, previous.height + 1);
    let expired_id = expired.operation.operation_id().unwrap();
    let applied_id = applied.operation.operation_id().unwrap();
    let proposal = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![applied, expired]),
            ProposalLimits::default(),
        )
        .unwrap();
    let snapshot = LedgerStateCodec::decode(NETWORK, &proposal.executed.transition.state).unwrap();
    let restored = Ledger::from_snapshot(NETWORK, snapshot).unwrap();

    assert_eq!(proposal.included, vec![expired_id, applied_id]);
    assert_eq!(proposal.executed.receipts.len(), 2);
    assert_eq!(
        proposal.executed.receipts[0].outcome,
        OperationOutcome::Expired
    );
    assert_eq!(
        proposal.executed.receipts[1].outcome,
        OperationOutcome::Applied
    );
    assert_eq!(restored.nonce(sender), 2);
    assert_eq!(restored.balance(TOKEN, sender), 9_899);
    assert_eq!(restored.balance(TOKEN, RECIPIENT), 100);
    assert_eq!(restored.balance(TOKEN, TREASURY), 1);
}

#[test]
fn proposal_caps_expired_cleanup_without_displacing_an_independent_payment() {
    let expired_key = sender_key();
    let active_key = SigningKey::from_bytes(&[43; 32]);
    let expired_sender = AccountId::new(expired_key.verifying_key().to_bytes());
    let active_sender = AccountId::new(active_key.verifying_key().to_bytes());
    let mut ledger = funded_ledger(expired_sender);
    ledger.mint(ISSUER, TOKEN, active_sender, 1_000).unwrap();
    let previous_state = LedgerStateCodec::encode(&ledger.snapshot()).unwrap();
    let previous = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([11; 32]),
        state_root: state_root(&previous_state),
        validator_set_hash: ValidatorSetHash::new([12; 32]),
    };
    let first_expired = signed_payment_until(&expired_key, RECIPIENT, 0, 15, 10, 1, 100);
    let second_expired = signed_payment_until(&expired_key, RECIPIENT, 1, 16, 10, 1, 100);
    let active = signed_payment_until(&active_key, RECIPIENT, 0, 17, 100, 1, 101);
    let first_expired_id = first_expired.operation.operation_id().unwrap();
    let second_expired_id = second_expired.operation.operation_id().unwrap();
    let active_id = active.operation.operation_id().unwrap();
    let proposal = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![first_expired, second_expired, active]),
            ProposalLimits {
                max_expired_operations: 1,
                ..ProposalLimits::default()
            },
        )
        .unwrap();

    assert_eq!(proposal.included.len(), 2);
    assert!(proposal.included.contains(&first_expired_id));
    assert!(proposal.included.contains(&active_id));
    assert_eq!(proposal.deferred, vec![second_expired_id]);
    assert_eq!(
        proposal
            .executed
            .receipts
            .iter()
            .filter(|receipt| receipt.outcome == OperationOutcome::Expired)
            .count(),
        1
    );
}

#[test]
fn proposal_rejects_invalid_expired_cleanup_quotas() {
    let (_, previous_state, previous) = initial_state();
    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).build_proposal(
            previous,
            &previous_state,
            &[],
            ProposalLimits {
                max_expired_operations: 0,
                ..ProposalLimits::default()
            },
        ),
        Err(RuntimeError::InvalidProposalLimits)
    );
    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).build_proposal(
            previous,
            &previous_state,
            &[],
            ProposalLimits {
                max_operations: 2,
                max_expired_operations: 2,
                ..ProposalLimits::default()
            },
        ),
        Err(RuntimeError::InvalidProposalLimits)
    );
}
