use std::{
    fs,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, NetworkId,
    OperationKind, OperationReceipt, SignatureBytes, SignedOperation, Transfer,
};
use payment_idempotency_core::{
    ClientRequestKey, PaymentIntent, PaymentRequestId, ReconciliationPageLimit, RequestDigest,
    ReservationState, TenantId,
};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use payment_reconciliation_service_core::{
    ReconciliationClock, ReconciliationCycleOutcome, ReconciliationCycleReport,
    ReconciliationTelemetry, ReconciliationTelemetryEvent, ReconciliationWorker,
    ReconciliationWorkerConfig, RetryPolicy,
};
use payment_reconciliation_work_core::{LeaseDuration, WorkerId};
use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexBase};
use receipt_index_lmdb::{LmdbReceiptIndex, ReceiptIndexStoreOptions};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([71; 32]);
const ACCOUNT: AccountId = AccountId::new([72; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-worker-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn checkpoint(height: u64, marker: u8) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height,
        block_hash: BlockHash::new([marker; 32]),
        state_root: StateRoot::new([marker.wrapping_add(1); 32]),
        validator_set_hash: ValidatorSetHash::new([73; 32]),
    }
}

fn intent(marker: u8) -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([74; 32]),
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
            asset: AssetId::new([75; 32]),
            from: ACCOUNT,
            to: AccountId::new([76; 32]),
            amount: u128::from(nonce) + 10,
            fee: 1,
            nonce,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: ACCOUNT,
            signature: SignatureBytes::new([77; 64]),
        },
        fee_payer_authorization: None,
    }
}

fn prepare(
    store: &LmdbPaymentIdempotencyStore,
    marker: u8,
    operation_index: u64,
) -> (PaymentIntent, OperationReceipt) {
    let payment = intent(marker);
    store.reserve(payment, 10).unwrap();
    let assigned = store
        .assign_next_nonce(payment.request_id, payment.request_digest, 0, 11)
        .unwrap();
    let nonce = assigned.reservation.state().nonce().unwrap();
    let operation = signed(payment, nonce);
    let operation_id = operation.operation.operation_id().unwrap();
    store
        .prepare_submission(payment.request_id, payment.request_digest, &operation, 12)
        .unwrap();
    (
        payment,
        OperationReceipt {
            operation_id,
            account: ACCOUNT,
            idempotency_key: payment.ledger_idempotency_key,
            nonce,
            operation_index,
            kind: OperationKind::Transfer,
            outcome: ledger_core::OperationOutcome::Applied,
        },
    )
}

#[derive(Debug)]
struct ManualClock(AtomicU64);

impl ManualClock {
    fn set(&self, now_ms: u64) {
        self.0.store(now_ms, Ordering::Release);
    }
}

impl ReconciliationClock for ManualClock {
    type Error = ();

    fn now_ms(&self) -> Result<u64, Self::Error> {
        Ok(self.0.load(Ordering::Acquire))
    }
}

#[derive(Debug, Default)]
struct CapturingTelemetry(Mutex<Vec<ReconciliationTelemetryEvent>>);

impl ReconciliationTelemetry for CapturingTelemetry {
    fn record(&self, event: ReconciliationTelemetryEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn config() -> ReconciliationWorkerConfig {
    ReconciliationWorkerConfig {
        lease_duration: LeaseDuration::new(500).unwrap(),
        page_limit: ReconciliationPageLimit::new(10).unwrap(),
        retry_policy: RetryPolicy::new(100, 20, 1_000, 3).unwrap(),
    }
}

fn run_cycle(
    payments: &LmdbPaymentIdempotencyStore,
    receipts: &LmdbReceiptIndex,
    clock: &ManualClock,
    telemetry: &CapturingTelemetry,
    owner: WorkerId,
) -> ReconciliationCycleOutcome {
    ReconciliationWorker::new(payments, receipts, payments, clock, telemetry, config())
        .run_cycle(owner)
        .unwrap()
}

fn assert_processed(
    payments: &LmdbPaymentIdempotencyStore,
    receipts: &LmdbReceiptIndex,
    clock: &ManualClock,
    telemetry: &CapturingTelemetry,
    owner_marker: u8,
    expected: ReconciliationCycleReport,
) {
    assert_eq!(
        run_cycle(
            payments,
            receipts,
            clock,
            telemetry,
            WorkerId::new([owner_marker; 32]).unwrap(),
        ),
        ReconciliationCycleOutcome::Processed(expected)
    );
}

fn assert_finalized_and_empty(
    payments: &LmdbPaymentIdempotencyStore,
    payment_intents: &[PaymentIntent],
) {
    for payment in payment_intents {
        assert!(matches!(
            payments.get(payment.request_id).unwrap().unwrap().state(),
            ReservationState::Finalized(_)
        ));
    }
    assert!(
        payments
            .scan_reconcilable_after(None, config().page_limit)
            .unwrap()
            .reservations
            .is_empty()
    );
}

#[test]
fn pending_retry_is_durable_not_early_and_finalizes_after_restart() {
    let payment_path = directory("payments");
    let receipt_path = directory("receipts");
    let clock = ManualClock(AtomicU64::new(1_000));
    let telemetry = CapturingTelemetry::default();
    let receipts =
        LmdbReceiptIndex::open(&receipt_path, NETWORK, ReceiptIndexStoreOptions::default())
            .unwrap();
    let base = checkpoint(10, 80);
    let first_checkpoint = checkpoint(11, 81);
    let second_checkpoint = checkpoint(12, 82);
    receipts
        .initialize(ReceiptIndexBase {
            network: NETWORK,
            checkpoint: base,
            next_operation_index: 0,
        })
        .unwrap();

    let (first_payment, second_payment, second_receipt) = {
        let payments = LmdbPaymentIdempotencyStore::open(
            &payment_path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
        let (first_payment, first_receipt) = prepare(&payments, 1, 0);
        let (second_payment, second_receipt) = prepare(&payments, 2, 1);
        receipts
            .commit_finalized(&FinalizedReceiptBlock {
                network: NETWORK,
                previous: base,
                checkpoint: first_checkpoint,
                receipts: vec![first_receipt],
            })
            .unwrap();
        assert_processed(
            &payments,
            &receipts,
            &clock,
            &telemetry,
            1,
            ReconciliationCycleReport {
                scanned: 2,
                finalized: 1,
                pending: 1,
                failed: 0,
                alerts: 0,
            },
        );
        clock.set(1_050);
        assert_processed(
            &payments,
            &receipts,
            &clock,
            &telemetry,
            2,
            ReconciliationCycleReport::default(),
        );
        (first_payment, second_payment, second_receipt)
    };

    receipts
        .commit_finalized(&FinalizedReceiptBlock {
            network: NETWORK,
            previous: first_checkpoint,
            checkpoint: second_checkpoint,
            receipts: vec![second_receipt],
        })
        .unwrap();
    clock.set(1_100);
    {
        let payments = LmdbPaymentIdempotencyStore::open(
            &payment_path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
        assert_processed(
            &payments,
            &receipts,
            &clock,
            &telemetry,
            3,
            ReconciliationCycleReport {
                scanned: 1,
                finalized: 1,
                pending: 0,
                failed: 0,
                alerts: 0,
            },
        );
        assert_finalized_and_empty(&payments, &[first_payment, second_payment]);
    }
    assert_eq!(telemetry.0.lock().unwrap().len(), 3);
    drop(receipts);
    fs::remove_dir_all(payment_path).unwrap();
    fs::remove_dir_all(receipt_path).unwrap();
}
