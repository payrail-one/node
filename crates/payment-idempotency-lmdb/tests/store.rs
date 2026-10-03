use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, NetworkId,
    OperationKind, OperationReceipt, SignatureBytes, SignedOperation, Transfer,
};
use payment_idempotency_core::{
    ClientRequestKey, IndeterminateReason, MutationOutcome, PaymentIdempotencyError, PaymentIntent,
    PaymentRequestId, RejectionCode, RequestDigest, ReservationState, TenantId,
};
use payment_idempotency_lmdb::{
    LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreError, PaymentIdempotencyStoreOptions,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([2; 32]);
const ACCOUNT: AccountId = AccountId::new([3; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-idempotency-lmdb-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn request_id(tenant: u8, request: u8) -> PaymentRequestId {
    PaymentRequestId {
        tenant: TenantId::new([tenant; 32]),
        account: ACCOUNT,
        client_key: ClientRequestKey::new([request; 32]),
    }
}

fn intent(id: PaymentRequestId, marker: u8) -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: id,
        request_digest: RequestDigest::new([marker; 32]),
        ledger_idempotency_key: IdempotencyKey::new([marker.wrapping_add(1); 32]),
    }
}

fn signed(intent: PaymentIntent, nonce: u64, amount: u128) -> SignedOperation {
    SignedOperation {
        operation: AuthorizedOperation::Transfer(Transfer {
            network: intent.network,
            idempotency_key: intent.ledger_idempotency_key,
            asset: AssetId::new([20; 32]),
            from: intent.request_id.account,
            to: AccountId::new([21; 32]),
            amount,
            fee: 1,
            nonce,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: intent.request_id.account,
            signature: SignatureBytes::new([22; 64]),
        },
        fee_payer_authorization: None,
    }
}

fn checkpoint() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 10,
        block_hash: BlockHash::new([30; 32]),
        state_root: StateRoot::new([31; 32]),
        validator_set_hash: ValidatorSetHash::new([32; 32]),
    }
}

#[test]
fn journal_survives_restart_and_exact_retry_returns_final_result() {
    let path = directory("restart");
    let id = request_id(4, 5);
    let payment = intent(id, 6);
    let operation = signed(payment, 7, 50);
    let operation_id = operation.operation.operation_id().unwrap();
    let receipt = OperationReceipt {
        operation_id,
        account: ACCOUNT,
        idempotency_key: payment.ledger_idempotency_key,
        nonce: 7,
        operation_index: 100,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    };
    {
        let store = LmdbPaymentIdempotencyStore::open(
            &path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
        assert_eq!(
            store.reserve(payment, 100).unwrap().outcome,
            MutationOutcome::Applied
        );
        assert_eq!(
            store
                .assign_next_nonce(id, payment.request_digest, 7, 101)
                .unwrap()
                .reservation
                .state()
                .nonce(),
            Some(7)
        );
        store
            .prepare_submission(id, payment.request_digest, &operation, 102)
            .unwrap();
        store
            .mark_indeterminate(
                id,
                payment.request_digest,
                IndeterminateReason::BroadcastOutcomeUnknown,
                103,
            )
            .unwrap();
        store
            .finalize(id, payment.request_digest, checkpoint(), receipt, 104)
            .unwrap();
    }
    let reopened = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let retry = reopened.reserve(payment, 200).unwrap();
    assert_eq!(retry.outcome, MutationOutcome::ExistingSame);
    assert!(matches!(
        retry.reservation.state(),
        ReservationState::Finalized(finalized)
            if finalized.receipt == receipt && finalized.submission.envelope.len() > 100
    ));
    assert_eq!(
        reopened
            .prepare_submission(id, payment.request_digest, &operation, 201)
            .unwrap()
            .outcome,
        MutationOutcome::ExistingSame
    );
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn nonce_allocation_is_shared_across_tenants_and_tracks_ledger_floor() {
    let path = directory("nonces");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let first = intent(request_id(4, 1), 40);
    let second = intent(request_id(5, 2), 50);
    let third = intent(request_id(6, 3), 60);
    for payment in [first, second, third] {
        store.reserve(payment, 100).unwrap();
    }
    assert_eq!(
        store
            .assign_next_nonce(first.request_id, first.request_digest, 5, 101)
            .unwrap()
            .reservation
            .state()
            .nonce(),
        Some(5)
    );
    assert_eq!(
        store
            .assign_next_nonce(second.request_id, second.request_digest, 5, 102)
            .unwrap()
            .reservation
            .state()
            .nonce(),
        Some(6)
    );
    assert_eq!(
        store
            .assign_next_nonce(third.request_id, third.request_digest, 20, 103)
            .unwrap()
            .reservation
            .state()
            .nonce(),
        Some(20)
    );
    assert_eq!(
        store
            .assign_next_nonce(first.request_id, first.request_digest, 99, 104)
            .unwrap()
            .reservation
            .state()
            .nonce(),
        Some(5)
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn conflicts_fail_atomically_without_replacing_the_reserved_operation() {
    let path = directory("conflicts");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let payment = intent(request_id(4, 5), 6);
    store.reserve(payment, 100).unwrap();
    let mut conflict = payment;
    conflict.request_digest = RequestDigest::new([99; 32]);
    assert_eq!(
        store.reserve(conflict, 101),
        Err(PaymentIdempotencyStoreError::Domain(
            PaymentIdempotencyError::RequestConflict
        ))
    );
    store
        .assign_next_nonce(payment.request_id, payment.request_digest, 7, 102)
        .unwrap();
    let wrong = signed(payment, 8, 50);
    assert_eq!(
        store.prepare_submission(payment.request_id, payment.request_digest, &wrong, 103),
        Err(PaymentIdempotencyStoreError::Domain(
            PaymentIdempotencyError::NonceMismatch
        ))
    );
    assert!(matches!(
        store.get(payment.request_id).unwrap().unwrap().state(),
        ReservationState::NonceAssigned(7)
    ));
    store
        .prepare_submission(
            payment.request_id,
            payment.request_digest,
            &signed(payment, 7, 50),
            104,
        )
        .unwrap();
    assert_eq!(
        store.reject_before_submission(
            payment.request_id,
            payment.request_digest,
            RejectionCode::PolicyDenied,
            105,
        ),
        Err(PaymentIdempotencyStoreError::Domain(
            PaymentIdempotencyError::InvalidTransition
        ))
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn network_binding_and_minimum_map_size_fail_closed() {
    let path = directory("network");
    {
        LmdbPaymentIdempotencyStore::open(
            &path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
    }
    assert_eq!(
        LmdbPaymentIdempotencyStore::open(
            &path,
            OTHER_NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap_err(),
        PaymentIdempotencyStoreError::WrongNetwork
    );
    fs::remove_dir_all(path).unwrap();

    let small = directory("small");
    assert_eq!(
        LmdbPaymentIdempotencyStore::open(
            &small,
            NETWORK,
            PaymentIdempotencyStoreOptions {
                map_size: 1024,
                max_readers: 4,
            },
        )
        .unwrap_err(),
        PaymentIdempotencyStoreError::MapSizeTooSmall
    );
    assert!(!small.exists());
}
