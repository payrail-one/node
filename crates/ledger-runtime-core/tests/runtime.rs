mod support;

use ed25519_dalek::SigningKey;
use ledger_core::{
    AccountId, ContractDeploy, IdempotencyKey, Ledger, LedgerError, SignatureBytes,
    SignedOperation, Transfer,
};
use ledger_runtime_core::{
    CompactLedgerBlockManifest, LedgerAuthenticatedState, LedgerBlockCodec, LedgerBlockExecutor,
    LedgerStateCodec, MAX_BLOCK_OPERATIONS, ProposalLimits, ProposalRejection, RuntimeError,
    StateCommitmentPolicy, StateCommitmentScheme, authenticated_state_root, block_hash,
    block_payload_hash, state_root,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, ValidatorSetHash};
use transaction_auth_ed25519::Ed25519Verifier;
use transaction_protocol::SignedOperationCodec;

use support::*;

fn signed_transfer(
    key: &SigningKey,
    nonce: u64,
    idempotency_byte: u8,
    amount: u128,
    fee: u128,
) -> SignedOperation {
    signed_payment_until(key, RECIPIENT, nonce, idempotency_byte, amount, fee, 10_000)
}

fn signed_payment(
    key: &SigningKey,
    recipient: AccountId,
    nonce: u64,
    idempotency_byte: u8,
    amount: u128,
    fee: u128,
) -> SignedOperation {
    signed_payment_until(key, recipient, nonce, idempotency_byte, amount, fee, 10_000)
}

#[test]
fn state_codec_round_trips_validated_consensus_state() {
    let sender = AccountId::new(sender_key().verifying_key().to_bytes());
    let snapshot = funded_ledger(sender).snapshot();
    let encoded = LedgerStateCodec::encode(&snapshot).unwrap();
    let decoded = LedgerStateCodec::decode(NETWORK, &encoded).unwrap();

    assert_eq!(decoded, snapshot);
    assert_eq!(LedgerStateCodec::encode(&decoded).unwrap(), encoded);
    assert_eq!(&encoded[..16], b"ledger.state.v2\0");
}

#[test]
fn contract_state_uses_v3_without_rewriting_contract_free_v2_state() {
    let owner = AccountId::new(sender_key().verifying_key().to_bytes());
    let mut ledger = funded_ledger(owner);
    let mut code = b"PRC1".to_vec();
    code.extend([1, 4]);
    code.extend(b"noop");
    code.extend(1_u16.to_be_bytes());
    code.push(0);
    ledger
        .deploy_contract(
            owner,
            ContractDeploy {
                network: NETWORK,
                idempotency_key: IdempotencyKey::new([77; 32]),
                asset: TOKEN,
                owner,
                salt: [78; 32],
                code,
                fee: 1,
                nonce: 0,
                valid_until_height: 100,
            },
        )
        .unwrap();

    let encoded = LedgerStateCodec::encode(&ledger.snapshot()).unwrap();
    assert_eq!(&encoded[..16], b"ledger.state.v3\0");
    let decoded = LedgerStateCodec::decode(NETWORK, &encoded).unwrap();
    assert_eq!(LedgerStateCodec::encode(&decoded).unwrap(), encoded);
}

#[test]
fn real_signed_operations_execute_in_consensus_order() {
    let key = sender_key();
    let (sender, previous_state, previous) = initial_state();
    let payload = LedgerBlockCodec::encode(&[
        signed_transfer(&key, 0, 11, 1_000, 5),
        signed_transfer(&key, 1, 12, 500, 3),
    ])
    .unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);

    let executed = executor
        .execute(previous, &previous_state, &payload)
        .unwrap();
    let restored = Ledger::from_snapshot(
        NETWORK,
        LedgerStateCodec::decode(NETWORK, &executed.transition.state).unwrap(),
    )
    .unwrap();

    assert_eq!(executed.receipts.len(), 2);
    assert_eq!(restored.balance(TOKEN, sender), 8_492);
    assert_eq!(restored.balance(TOKEN, RECIPIENT), 1_500);
    assert_eq!(restored.balance(TOKEN, TREASURY), 8);
    assert_eq!(restored.nonce(sender), 2);
    assert_eq!(
        executed.transition.commitment.block_hash,
        block_hash(previous.block_hash, &payload)
    );
    assert_eq!(
        executed.transition.commitment.state_root,
        state_root(&executed.transition.state)
    );
}

#[test]
fn one_failed_operation_rejects_the_whole_block_without_mutating_input() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let original = previous_state.clone();
    let payload = LedgerBlockCodec::encode(&[
        signed_transfer(&key, 0, 21, 100, 1),
        signed_transfer(&key, 1, 22, 50_000, 1),
    ])
    .unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);

    assert_eq!(
        executor.execute(previous, &previous_state, &payload),
        Err(RuntimeError::ExecutionFailed {
            operation_index: 1,
            source: LedgerError::InsufficientBalance,
        })
    );
    assert_eq!(previous_state, original);

    let valid = LedgerBlockCodec::encode(&[signed_transfer(&key, 0, 21, 100, 1)]).unwrap();
    assert!(executor.execute(previous, &previous_state, &valid).is_ok());
}

#[test]
fn state_root_and_signature_tampering_are_rejected() {
    let key = sender_key();
    let (_, previous_state, mut previous) = initial_state();
    let valid = signed_transfer(&key, 0, 31, 100, 1);
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);

    previous.state_root = state_sync_core::StateRoot::new([99; 32]);
    let payload = LedgerBlockCodec::encode(std::slice::from_ref(&valid)).unwrap();
    assert_eq!(
        executor.execute(previous, &previous_state, &payload),
        Err(RuntimeError::PreviousStateRootMismatch)
    );

    let (_, previous_state, previous) = initial_state();
    let mut forged = valid;
    forged.sender_authorization.signature = SignatureBytes::new([0; 64]);
    let forged_payload = LedgerBlockCodec::encode(&[forged]).unwrap();
    assert_eq!(
        executor.execute(previous, &previous_state, &forged_payload),
        Err(RuntimeError::ExecutionFailed {
            operation_index: 0,
            source: LedgerError::InvalidSignature,
        })
    );
}

#[test]
fn commitment_policy_switches_once_at_the_configured_height() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let policy = StateCommitmentPolicy::authenticated_state_v1_from(101);
    assert_eq!(
        policy.scheme_at(100),
        StateCommitmentScheme::CanonicalStateV1
    );
    assert_eq!(
        policy.scheme_at(101),
        StateCommitmentScheme::AuthenticatedStateV1
    );
    assert_eq!(
        policy.scheme_at(u64::MAX),
        StateCommitmentScheme::AuthenticatedStateV1
    );
    let executor =
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).with_state_commitment_policy(policy);
    let first_payload = LedgerBlockCodec::encode(&[signed_transfer(&key, 0, 35, 100, 1)]).unwrap();
    let first = executor
        .execute(previous, &previous_state, &first_payload)
        .unwrap();
    let first_snapshot = LedgerStateCodec::decode(NETWORK, &first.transition.state).unwrap();

    assert_eq!(
        first.transition.commitment.state_root,
        authenticated_state_root(&first_snapshot).unwrap()
    );
    assert_ne!(
        first.transition.commitment.state_root,
        state_root(&first.transition.state)
    );

    let first_checkpoint = FinalizedCheckpoint {
        height: 101,
        block_hash: first.transition.commitment.block_hash,
        state_root: first.transition.commitment.state_root,
        validator_set_hash: previous.validator_set_hash,
    };
    let second_payload = LedgerBlockCodec::encode(&[signed_transfer(&key, 1, 36, 100, 1)]).unwrap();
    let second = executor
        .execute(first_checkpoint, &first.transition.state, &second_payload)
        .unwrap();
    let second_snapshot = LedgerStateCodec::decode(NETWORK, &second.transition.state).unwrap();
    assert_eq!(
        second.transition.commitment.state_root,
        authenticated_state_root(&second_snapshot).unwrap()
    );

    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).execute(
            first_checkpoint,
            &first.transition.state,
            &second_payload,
        ),
        Err(RuntimeError::PreviousStateRootMismatch)
    );
}

#[test]
fn incremental_execution_matches_rebuild_across_commitment_activation() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let previous_snapshot = LedgerStateCodec::decode(NETWORK, &previous_state).unwrap();
    let mut authenticated =
        LedgerAuthenticatedState::create(NETWORK, previous.height, &previous_snapshot).unwrap();
    let original_authenticated_root = authenticated.root();
    let policy = StateCommitmentPolicy::authenticated_state_v1_from(101);
    let executor =
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).with_state_commitment_policy(policy);
    let payload = LedgerBlockCodec::encode(&[signed_transfer(&key, 0, 37, 100, 1)]).unwrap();

    let prepared = executor
        .execute_incremental(previous, &previous_state, &payload, &authenticated)
        .unwrap();
    let rebuilt = executor
        .execute(previous, &previous_state, &payload)
        .unwrap();

    assert_eq!(prepared.executed(), &rebuilt);
    assert_eq!(authenticated.latest_height(), 100);
    assert_eq!(authenticated.root(), original_authenticated_root);

    let (first, update) = prepared.into_parts();
    let first_root = authenticated.commit(update).unwrap();
    assert_eq!(first_root, first.transition.commitment.state_root);

    let first_checkpoint = FinalizedCheckpoint {
        height: 101,
        block_hash: first.transition.commitment.block_hash,
        state_root: first.transition.commitment.state_root,
        validator_set_hash: previous.validator_set_hash,
    };
    let second_payload = LedgerBlockCodec::encode(&[signed_transfer(&key, 1, 38, 100, 1)]).unwrap();
    let second = executor
        .execute_incremental(
            first_checkpoint,
            &first.transition.state,
            &second_payload,
            &authenticated,
        )
        .unwrap();
    assert_eq!(
        second.executed(),
        &executor
            .execute(first_checkpoint, &first.transition.state, &second_payload)
            .unwrap()
    );
}

#[test]
fn incremental_execution_rejects_a_different_authenticated_predecessor() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let mut different = funded_ledger(AccountId::new(key.verifying_key().to_bytes()));
    different
        .transfer(
            AccountId::new(key.verifying_key().to_bytes()),
            Transfer {
                network: NETWORK,
                idempotency_key: IdempotencyKey::new([39; 32]),
                asset: TOKEN,
                from: AccountId::new(key.verifying_key().to_bytes()),
                to: RECIPIENT,
                amount: 1,
                fee: 0,
                nonce: 0,
                valid_until_height: 10_000,
            },
        )
        .unwrap();
    let authenticated =
        LedgerAuthenticatedState::create(NETWORK, previous.height, &different.snapshot()).unwrap();
    let payload = LedgerBlockCodec::encode(&[]).unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .with_state_commitment_policy(StateCommitmentPolicy::authenticated_state_v1_from(101));

    assert!(matches!(
        executor.execute_incremental(previous, &previous_state, &payload, &authenticated),
        Err(RuntimeError::PreviousStateRootMismatch)
    ));
    assert_eq!(authenticated.latest_height(), 100);
}

#[test]
fn block_codec_rejects_truncation_trailing_bytes_and_declared_oversize() {
    let operation = signed_transfer(&sender_key(), 0, 41, 100, 1);
    let encoded = LedgerBlockCodec::encode(&[operation]).unwrap();
    assert!(LedgerBlockCodec::decode(&encoded[..encoded.len() - 1]).is_err());

    let mut trailing = encoded.clone();
    trailing.push(0);
    assert_eq!(
        LedgerBlockCodec::decode(&trailing),
        Err(RuntimeError::TrailingBytes)
    );

    let mut oversized_envelope = encoded;
    oversized_envelope[20..24].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        LedgerBlockCodec::decode(&oversized_envelope),
        Err(RuntimeError::EnvelopeTooLarge)
    );

    let mut oversized_count = b"ledger.block.v1\0".to_vec();
    oversized_count.extend_from_slice(
        &u32::try_from(MAX_BLOCK_OPERATIONS + 1)
            .unwrap()
            .to_be_bytes(),
    );
    assert_eq!(
        LedgerBlockCodec::decode(&oversized_count),
        Err(RuntimeError::TooManyOperations)
    );
}

#[test]
fn compact_manifest_binds_the_exact_canonical_payload_and_operation_order() {
    let first = signed_transfer(&sender_key(), 0, 42, 100, 1);
    let second = signed_transfer(&sender_key(), 1, 43, 200, 2);
    let expected_ids = vec![
        first.operation.operation_id().unwrap(),
        second.operation.operation_id().unwrap(),
    ];
    let payload = LedgerBlockCodec::encode(&[first, second]).unwrap();
    let manifest = CompactLedgerBlockManifest::from_payload(&payload).unwrap();
    let encoded = manifest.encode();

    assert_eq!(manifest.payload_hash(), block_payload_hash(&payload));
    assert_eq!(manifest.operations(), expected_ids);
    assert!(encoded.len() < payload.len());
    assert_eq!(
        CompactLedgerBlockManifest::decode(&encoded).unwrap(),
        manifest
    );
    assert!(manifest.matches_payload(&payload).unwrap());

    let mut changed_payload = payload.clone();
    let last = changed_payload.last_mut().unwrap();
    *last ^= 1;
    assert!(!manifest.matches_payload(&changed_payload).unwrap());

    let mut changed_manifest = encoded;
    let last = changed_manifest.last_mut().unwrap();
    *last ^= 1;
    assert!(
        !CompactLedgerBlockManifest::decode(&changed_manifest)
            .unwrap()
            .matches_payload(&payload)
            .unwrap()
    );
}

#[test]
fn empty_block_is_a_valid_deterministic_state_transition() {
    let (_, previous_state, previous) = initial_state();
    let payload = LedgerBlockCodec::encode(&[]).unwrap();
    let executed = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .execute(previous, &previous_state, &payload)
        .unwrap();

    assert!(executed.receipts.is_empty());
    assert_eq!(executed.transition.state, previous_state);
    assert_ne!(
        executed.transition.commitment.block_hash,
        previous.block_hash
    );
}

#[test]
fn proposal_orders_sender_nonces_and_reproduces_runtime_state() {
    let key = sender_key();
    let (sender, previous_state, previous) = initial_state();
    let first = signed_transfer(&key, 0, 51, 100, 1);
    let second = signed_transfer(&key, 1, 52, 200, 2);
    let first_id = first.operation.operation_id().unwrap();
    let second_id = second.operation.operation_id().unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);

    let proposal = executor
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![second, first]),
            ProposalLimits::default(),
        )
        .unwrap();
    let snapshot = LedgerStateCodec::decode(NETWORK, &proposal.executed.transition.state).unwrap();
    let ledger = Ledger::from_snapshot(NETWORK, snapshot).unwrap();

    assert_eq!(proposal.included, vec![first_id, second_id]);
    assert!(proposal.deferred.is_empty());
    assert!(proposal.rejected.is_empty());
    assert_eq!(ledger.nonce(sender), 2);
    assert_eq!(ledger.balance(TOKEN, RECIPIENT), 300);
}

#[test]
fn incremental_proposal_matches_full_rebuild_and_remains_unpublished() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let snapshot = LedgerStateCodec::decode(NETWORK, &previous_state).unwrap();
    let mut authenticated =
        LedgerAuthenticatedState::create(NETWORK, previous.height, &snapshot).unwrap();
    let initial_root = authenticated.root();
    let operations = candidates(vec![
        signed_transfer(&key, 1, 54, 200, 2),
        signed_transfer(&key, 0, 53, 100, 1),
    ]);
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .with_state_commitment_policy(StateCommitmentPolicy::authenticated_state_v1_from(101));

    let prepared = executor
        .build_proposal_incremental(
            previous,
            &previous_state,
            &authenticated,
            &operations,
            ProposalLimits::default(),
        )
        .unwrap();
    let rebuilt = executor
        .build_proposal(
            previous,
            &previous_state,
            &operations,
            ProposalLimits::default(),
        )
        .unwrap();

    assert_eq!(prepared.proposal(), &rebuilt);
    assert_eq!(authenticated.root(), initial_root);
    assert_eq!(authenticated.latest_height(), previous.height);

    let (proposal, update) = prepared.into_parts();
    assert_eq!(
        authenticated.commit(update).unwrap(),
        proposal.executed.transition.commitment.state_root
    );
}

#[test]
fn proposal_rejects_permanent_failures_and_defers_state_dependent_failures() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let mut forged = signed_transfer(&key, 0, 61, 100, 1);
    forged.sender_authorization.signature = SignatureBytes::new([0; 64]);
    let forged_id = forged.operation.operation_id().unwrap();
    let future = signed_transfer(&key, 5, 62, 100, 1);
    let future_id = future.operation.operation_id().unwrap();
    let executor = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier);

    let proposal = executor
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![future, forged]),
            ProposalLimits::default(),
        )
        .unwrap();

    assert!(proposal.included.is_empty());
    assert_eq!(proposal.deferred, vec![future_id]);
    assert_eq!(proposal.rejected.len(), 1);
    assert_eq!(proposal.rejected[0].operation, forged_id);
    assert_eq!(
        proposal.rejected[0].reason,
        ProposalRejection::PermanentLedgerFailure
    );
    assert_eq!(proposal.executed.transition.state, previous_state);
}

#[test]
fn proposal_bounds_payload_without_dropping_deferred_candidates() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let first = signed_transfer(&key, 0, 71, 100, 1);
    let second = signed_transfer(&key, 1, 72, 100, 1);
    let first_envelope = SignedOperationCodec::encode(&first).unwrap();
    let second_id = second.operation.operation_id().unwrap();
    let limits = ProposalLimits {
        max_payload_bytes: 20 + 4 + first_envelope.len(),
        ..ProposalLimits::default()
    };

    let proposal = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![second, first]),
            limits,
        )
        .unwrap();

    assert_eq!(proposal.included.len(), 1);
    assert_eq!(proposal.deferred, vec![second_id]);
    assert!(proposal.payload.len() <= limits.max_payload_bytes);
}

#[test]
fn proposal_requires_strictly_canonical_candidate_ids() {
    let key = sender_key();
    let (_, previous_state, previous) = initial_state();
    let mut input = candidates(vec![
        signed_transfer(&key, 0, 81, 100, 1),
        signed_transfer(&key, 1, 82, 100, 1),
    ]);
    input.swap(0, 1);

    assert_eq!(
        LedgerBlockExecutor::new(NETWORK, Ed25519Verifier).build_proposal(
            previous,
            &previous_state,
            &input,
            ProposalLimits::default(),
        ),
        Err(RuntimeError::NonCanonicalCandidates)
    );
}

#[test]
fn bounded_second_pass_resolves_an_incoming_payment_dependency() {
    let first_key = SigningKey::from_bytes(&[20; 32]);
    let second_key = SigningKey::from_bytes(&[21; 32]);
    let first_account = AccountId::new(first_key.verifying_key().to_bytes());
    let second_account = AccountId::new(second_key.verifying_key().to_bytes());
    let (spender_key, spender, funder_key, funder) = if first_account < second_account {
        (first_key, first_account, second_key, second_account)
    } else {
        (second_key, second_account, first_key, first_account)
    };
    let ledger = funded_ledger(funder);
    let previous_state = LedgerStateCodec::encode(&ledger.snapshot()).unwrap();
    let previous = FinalizedCheckpoint {
        height: 200,
        block_hash: BlockHash::new([90; 32]),
        state_root: state_root(&previous_state),
        validator_set_hash: ValidatorSetHash::new([91; 32]),
    };
    let spend = signed_payment(&spender_key, RECIPIENT, 0, 91, 200, 1);
    let funding = signed_payment(&funder_key, spender, 0, 92, 500, 1);
    let spend_id = spend.operation.operation_id().unwrap();
    let funding_id = funding.operation.operation_id().unwrap();

    let proposal = LedgerBlockExecutor::new(NETWORK, Ed25519Verifier)
        .build_proposal(
            previous,
            &previous_state,
            &candidates(vec![funding, spend]),
            ProposalLimits::default(),
        )
        .unwrap();
    let snapshot = LedgerStateCodec::decode(NETWORK, &proposal.executed.transition.state).unwrap();
    let restored = Ledger::from_snapshot(NETWORK, snapshot).unwrap();

    assert_eq!(proposal.included, vec![funding_id, spend_id]);
    assert!(proposal.deferred.is_empty());
    assert_eq!(restored.balance(TOKEN, spender), 299);
    assert_eq!(restored.balance(TOKEN, RECIPIENT), 200);
}
