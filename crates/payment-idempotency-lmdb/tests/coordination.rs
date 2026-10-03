use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, NetworkId,
    SignatureBytes, SignedOperation, Transfer,
};
use payment_idempotency_core::{
    ClientRequestKey, PaymentIntent, PaymentRequestId, ReconciliationPageLimit, RequestDigest,
    TenantId,
};
use payment_idempotency_lmdb::{
    LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreError, PaymentIdempotencyStoreOptions,
};
use payment_reconciliation_work_core::{
    DeferReason, LeaseAcquireOutcome, LeaseDuration, ReconciliationWorkError,
    ReconciliationWorkStore, WorkerId,
};

const NETWORK: NetworkId = NetworkId::new([61; 32]);
const ACCOUNT: AccountId = AccountId::new([62; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory() -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-coordination-{}-{sequence}",
        std::process::id()
    ))
}

fn intent(marker: u8) -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([63; 32]),
            account: ACCOUNT,
            client_key: ClientRequestKey::new([marker; 32]),
        },
        request_digest: RequestDigest::new([marker.wrapping_add(10); 32]),
        ledger_idempotency_key: IdempotencyKey::new([marker.wrapping_add(20); 32]),
    }
}

fn signed(payment: PaymentIntent, nonce: u64) -> SignedOperation {
    SignedOperation {
        operation: AuthorizedOperation::Transfer(Transfer {
            network: NETWORK,
            idempotency_key: payment.ledger_idempotency_key,
            asset: AssetId::new([64; 32]),
            from: ACCOUNT,
            to: AccountId::new([65; 32]),
            amount: 10,
            fee: 1,
            nonce,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: ACCOUNT,
            signature: SignatureBytes::new([66; 64]),
        },
        fee_payer_authorization: None,
    }
}

fn prepare(store: &LmdbPaymentIdempotencyStore, marker: u8) -> PaymentIntent {
    let payment = intent(marker);
    store.reserve(payment, 10).unwrap();
    let assigned = store
        .assign_next_nonce(payment.request_id, payment.request_digest, 0, 11)
        .unwrap();
    let nonce = assigned.reservation.state().nonce().unwrap();
    store
        .prepare_submission(
            payment.request_id,
            payment.request_digest,
            &signed(payment, nonce),
            12,
        )
        .unwrap();
    payment
}

#[test]
fn lease_takeover_fences_stale_owner_and_retry_schedule_survives_restart() {
    let path = directory();
    let first = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    prepare(&first, 1);
    prepare(&first, 2);
    let duration = LeaseDuration::new(100).unwrap();
    let owner_one = WorkerId::new([1; 32]).unwrap();
    let owner_two = WorkerId::new([2; 32]).unwrap();
    let lease_one = match first.acquire_lease(owner_one, 100, duration).unwrap() {
        LeaseAcquireOutcome::Acquired(lease) => lease,
        LeaseAcquireOutcome::Busy { .. } => panic!("first worker must acquire the lease"),
    };
    assert_eq!(lease_one.token.get(), 1);
    assert_eq!(
        first.acquire_lease(owner_two, 150, duration).unwrap(),
        LeaseAcquireOutcome::Busy { expires_at_ms: 200 }
    );
    let limit = ReconciliationPageLimit::new(10).unwrap();
    let jobs = first.due_work(lease_one, 150, limit).unwrap();
    assert_eq!(jobs.len(), 2);
    first
        .defer(lease_one, jobs[0], DeferReason::Pending, 300, 150)
        .unwrap();

    let lease_two = match first.acquire_lease(owner_two, 200, duration).unwrap() {
        LeaseAcquireOutcome::Acquired(lease) => lease,
        LeaseAcquireOutcome::Busy { .. } => panic!("expired lease must permit takeover"),
    };
    assert_eq!(lease_two.token.get(), 2);
    assert_eq!(
        first.renew_lease(lease_one, 200, duration),
        Err(PaymentIdempotencyStoreError::Work(
            ReconciliationWorkError::StaleLease
        ))
    );
    let takeover_jobs = first.due_work(lease_two, 200, limit).unwrap();
    assert_eq!(takeover_jobs.len(), 1);
    assert_eq!(
        first.defer(lease_one, takeover_jobs[0], DeferReason::Error, 250, 200,),
        Err(PaymentIdempotencyStoreError::Work(
            ReconciliationWorkError::StaleLease
        ))
    );
    let failed = first
        .defer(lease_two, takeover_jobs[0], DeferReason::Error, 250, 200)
        .unwrap();
    assert_eq!(failed.consecutive_failures, 1);
    first.release_lease(lease_two).unwrap();
    drop(first);

    let reopened = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let lease_three = match reopened.acquire_lease(owner_one, 300, duration).unwrap() {
        LeaseAcquireOutcome::Acquired(lease) => lease,
        LeaseAcquireOutcome::Busy { .. } => panic!("released lease must be available"),
    };
    assert_eq!(lease_three.token.get(), 3);
    let recovered = reopened.due_work(lease_three, 300, limit).unwrap();
    assert_eq!(recovered.len(), 2);
    assert!(recovered.iter().any(|job| job.next_attempt_at_ms == 300));
    assert!(
        recovered
            .iter()
            .any(|job| job.next_attempt_at_ms == 250 && job.consecutive_failures == 1)
    );
    reopened.release_lease(lease_three).unwrap();
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}
