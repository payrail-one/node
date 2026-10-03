use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, NetworkId,
    OperationKind, OperationReceipt, SignatureBytes, SignedOperation, Transfer,
};
use payment_idempotency_core::{
    ClientRequestKey, IndeterminateReason, MAX_RECONCILIATION_PAGE_SIZE, MutationOutcome,
    PaymentIdempotencyError, PaymentIntent, PaymentRequestId, PaymentReservation,
    ReconciliationPageLimit, RejectionCode, RequestDigest, ReservationState, TenantId,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const ACCOUNT: AccountId = AccountId::new([2; 32]);
const LEDGER_KEY: IdempotencyKey = IdempotencyKey::new([3; 32]);
const DIGEST: RequestDigest = RequestDigest::new([4; 32]);

fn intent() -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([5; 32]),
            account: ACCOUNT,
            client_key: ClientRequestKey::new([6; 32]),
        },
        request_digest: DIGEST,
        ledger_idempotency_key: LEDGER_KEY,
    }
}

fn signed(nonce: u64, amount: u128) -> SignedOperation {
    SignedOperation {
        operation: AuthorizedOperation::Transfer(Transfer {
            network: NETWORK,
            idempotency_key: LEDGER_KEY,
            asset: AssetId::new([7; 32]),
            from: ACCOUNT,
            to: AccountId::new([8; 32]),
            amount,
            fee: 1,
            nonce,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: ACCOUNT,
            signature: SignatureBytes::new([9; 64]),
        },
        fee_payer_authorization: None,
    }
}

fn checkpoint() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 10,
        block_hash: BlockHash::new([10; 32]),
        state_root: StateRoot::new([11; 32]),
        validator_set_hash: ValidatorSetHash::new([12; 32]),
    }
}

#[test]
fn exact_envelope_survives_indeterminate_state_and_finalizes() {
    let mut reservation = PaymentReservation::new(intent(), 100);
    assert_eq!(
        reservation.assign_nonce(DIGEST, 7, 101).unwrap(),
        MutationOutcome::Applied
    );
    let operation = signed(7, 50);
    assert_eq!(
        reservation
            .prepare_submission(DIGEST, &operation, 102)
            .unwrap(),
        MutationOutcome::Applied
    );
    assert_eq!(
        reservation
            .prepare_submission(DIGEST, &operation, 103)
            .unwrap(),
        MutationOutcome::ExistingSame
    );
    assert_eq!(
        reservation
            .mark_indeterminate(DIGEST, IndeterminateReason::BroadcastOutcomeUnknown, 104)
            .unwrap(),
        MutationOutcome::Applied
    );
    let operation_id = operation.operation.operation_id().unwrap();
    let receipt = OperationReceipt {
        operation_id,
        account: ACCOUNT,
        idempotency_key: LEDGER_KEY,
        nonce: 7,
        operation_index: 99,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    };
    assert_eq!(
        reservation
            .finalize(DIGEST, checkpoint(), receipt, 105)
            .unwrap(),
        MutationOutcome::Applied
    );
    assert!(matches!(
        reservation.state(),
        ReservationState::Finalized(_)
    ));
}

#[test]
fn conflicting_retry_never_replaces_nonce_or_signed_operation() {
    let mut reservation = PaymentReservation::new(intent(), 100);
    reservation.assign_nonce(DIGEST, 7, 101).unwrap();
    assert_eq!(
        reservation.assign_nonce(DIGEST, 8, 102),
        Err(PaymentIdempotencyError::NonceMismatch)
    );
    reservation
        .prepare_submission(DIGEST, &signed(7, 50), 103)
        .unwrap();
    assert_eq!(
        reservation.prepare_submission(DIGEST, &signed(7, 51), 104),
        Err(PaymentIdempotencyError::OperationMismatch)
    );
    assert_eq!(
        reservation.mark_indeterminate(
            RequestDigest::new([99; 32]),
            IndeterminateReason::ReconciliationRequired,
            105,
        ),
        Err(PaymentIdempotencyError::RequestConflict)
    );
}

#[test]
fn rejection_is_terminal_and_timestamp_regression_fails() {
    let mut reservation = PaymentReservation::new(intent(), 100);
    assert_eq!(
        reservation.reject_before_submission(DIGEST, RejectionCode::PolicyDenied, 101),
        Ok(MutationOutcome::Applied)
    );
    assert_eq!(
        reservation.assign_nonce(DIGEST, 7, 102),
        Err(PaymentIdempotencyError::InvalidTransition)
    );
    assert_eq!(
        reservation.reject_before_submission(DIGEST, RejectionCode::PolicyDenied, 99),
        Err(PaymentIdempotencyError::TimestampRegression)
    );

    let mut assigned = PaymentReservation::new(intent(), 200);
    assigned.assign_nonce(DIGEST, 7, 201).unwrap();
    assert_eq!(
        assigned.reject_before_submission(DIGEST, RejectionCode::PolicyDenied, 202),
        Err(PaymentIdempotencyError::InvalidTransition)
    );
}

#[test]
fn reconciliation_page_limit_is_strictly_bounded() {
    assert_eq!(
        ReconciliationPageLimit::new(0),
        Err(PaymentIdempotencyError::InvalidPageLimit)
    );
    assert_eq!(
        ReconciliationPageLimit::new(MAX_RECONCILIATION_PAGE_SIZE + 1),
        Err(PaymentIdempotencyError::InvalidPageLimit)
    );
    assert_eq!(
        ReconciliationPageLimit::new(MAX_RECONCILIATION_PAGE_SIZE)
            .unwrap()
            .get(),
        MAX_RECONCILIATION_PAGE_SIZE
    );
}
