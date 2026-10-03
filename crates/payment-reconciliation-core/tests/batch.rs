use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, IdempotencyKey, NetworkId, OperationId,
    OperationKind, OperationReceipt, SignatureBytes, SignedOperation, Transfer,
};
use payment_idempotency_core::{
    ClientRequestKey, PaymentIntent, PaymentRequestId, ReconciliationPageLimit, RequestDigest,
    ReservationState, TenantId,
};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use payment_reconciliation_core::{
    PaymentFinalityReconciler, PaymentReconciliationError, PaymentReconciliationOutcome,
};
use receipt_index_core::{
    FinalizedReceiptBlock, IndexedFinalizedReceipt, ReceiptIndexBase, ReceiptIndexCursor,
    ReceiptIndexStore, ReceiptIndexWriteOutcome,
};
use receipt_index_lmdb::{LmdbReceiptIndex, ReceiptIndexStoreError, ReceiptIndexStoreOptions};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([41; 32]);
const ACCOUNT: AccountId = AccountId::new([42; 32]);
const PAYMENT_COUNT: u8 = 7;
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-reconciliation-batch-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn checkpoint(height: u64, marker: u8) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height,
        block_hash: BlockHash::new([marker; 32]),
        state_root: StateRoot::new([marker.wrapping_add(1); 32]),
        validator_set_hash: ValidatorSetHash::new([43; 32]),
    }
}

fn intent(marker: u8) -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([44; 32]),
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
            asset: AssetId::new([45; 32]),
            from: ACCOUNT,
            to: AccountId::new([46; 32]),
            amount: u128::from(nonce) + 1,
            fee: 1,
            nonce,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: ACCOUNT,
            signature: SignatureBytes::new([47; 64]),
        },
        fee_payer_authorization: None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectiveReceiptError {
    Injected,
    Store(ReceiptIndexStoreError),
}

struct SelectiveReceiptStore<'a> {
    inner: &'a LmdbReceiptIndex,
    failing_operation: OperationId,
}

impl ReceiptIndexStore for SelectiveReceiptStore<'_> {
    type Error = SelectiveReceiptError;

    fn network(&self) -> NetworkId {
        self.inner.network()
    }

    fn cursor(&self) -> Result<Option<ReceiptIndexCursor>, Self::Error> {
        ReceiptIndexStore::cursor(self.inner).map_err(SelectiveReceiptError::Store)
    }

    fn initialize(&self, base: ReceiptIndexBase) -> Result<(), Self::Error> {
        self.inner
            .initialize(base)
            .map_err(SelectiveReceiptError::Store)
    }

    fn commit_finalized(
        &self,
        block: &FinalizedReceiptBlock,
    ) -> Result<ReceiptIndexWriteOutcome, Self::Error> {
        self.inner
            .commit_finalized(block)
            .map_err(SelectiveReceiptError::Store)
    }

    fn finalized_receipt_by_operation_id(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<IndexedFinalizedReceipt>, Self::Error> {
        if operation_id == self.failing_operation {
            Err(SelectiveReceiptError::Injected)
        } else {
            self.inner
                .finalized_receipt_by_operation_id(operation_id)
                .map_err(SelectiveReceiptError::Store)
        }
    }
}

fn prepare_payments(
    payments: &LmdbPaymentIdempotencyStore,
) -> (Vec<PaymentIntent>, Vec<OperationReceipt>) {
    let mut payment_intents = Vec::new();
    let mut finalized_receipts = Vec::new();
    for marker in 0..PAYMENT_COUNT {
        let payment = intent(marker);
        payments.reserve(payment, 100).unwrap();
        let assigned = payments
            .assign_next_nonce(payment.request_id, payment.request_digest, 0, 101)
            .unwrap();
        let nonce = assigned.reservation.state().nonce().unwrap();
        let operation = signed(payment, nonce);
        let operation_id = operation.operation.operation_id().unwrap();
        payments
            .prepare_submission(payment.request_id, payment.request_digest, &operation, 102)
            .unwrap();
        payment_intents.push(payment);
        finalized_receipts.push(OperationReceipt {
            operation_id,
            account: ACCOUNT,
            idempotency_key: payment.ledger_idempotency_key,
            nonce,
            operation_index: u64::from(marker),
            kind: OperationKind::Transfer,
            outcome: ledger_core::OperationOutcome::Applied,
        });
    }
    (payment_intents, finalized_receipts)
}

fn run_selective_sweep(
    payments: &LmdbPaymentIdempotencyStore,
    receipts: &LmdbReceiptIndex,
    failing_operation: OperationId,
    limit: ReconciliationPageLimit,
) -> (BTreeSet<PaymentRequestId>, usize) {
    let selective = SelectiveReceiptStore {
        inner: receipts,
        failing_operation,
    };
    let reconciler = PaymentFinalityReconciler::new(payments, &selective);
    let mut cursor = None;
    let mut visited = BTreeSet::new();
    let mut failures = 0;
    loop {
        let batch = reconciler.reconcile_batch(cursor, limit, 200).unwrap();
        assert!(batch.items.len() <= limit.get());
        for item in batch.items {
            assert!(visited.insert(item.request_id));
            match item.result {
                Ok(PaymentReconciliationOutcome::Finalized(_)) => {}
                Err(PaymentReconciliationError::Receipt(SelectiveReceiptError::Injected)) => {
                    failures += 1;
                }
                other => panic!("unexpected reconciliation result: {other:?}"),
            }
        }
        cursor = batch.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    (visited, failures)
}

fn assert_recovered(
    payments: &LmdbPaymentIdempotencyStore,
    receipts: &LmdbReceiptIndex,
    payment_intents: &[PaymentIntent],
    finalized_checkpoint: FinalizedCheckpoint,
    limit: ReconciliationPageLimit,
) {
    let recovery = PaymentFinalityReconciler::new(payments, receipts)
        .reconcile_batch(None, limit, 300)
        .unwrap();
    assert_eq!(recovery.items.len(), 1);
    assert!(recovery.next_cursor.is_none());
    assert!(matches!(
        recovery.items[0].result,
        Ok(PaymentReconciliationOutcome::Finalized(_))
    ));
    assert!(
        payments
            .scan_reconcilable_after(None, limit)
            .unwrap()
            .reservations
            .is_empty()
    );
    for payment in payment_intents {
        assert!(matches!(
            payments
                .get(payment.request_id)
                .unwrap()
                .unwrap()
                .state(),
            ReservationState::Finalized(finalized)
                if finalized.checkpoint == finalized_checkpoint
        ));
    }
}

#[test]
fn bounded_sweep_isolates_item_errors_and_recovers_after_restart() {
    let payment_path = directory("payments");
    let receipt_path = directory("receipts");
    let finalized_checkpoint = checkpoint(11, 51);
    let limit = ReconciliationPageLimit::new(2).unwrap();
    let payment_intents = {
        let payments = LmdbPaymentIdempotencyStore::open(
            &payment_path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
        let receipts =
            LmdbReceiptIndex::open(&receipt_path, NETWORK, ReceiptIndexStoreOptions::default())
                .unwrap();
        let base = checkpoint(10, 50);
        receipts
            .initialize(ReceiptIndexBase {
                network: NETWORK,
                checkpoint: base,
                next_operation_index: 0,
            })
            .unwrap();
        let (payment_intents, finalized_receipts) = prepare_payments(&payments);
        receipts
            .commit_finalized(&FinalizedReceiptBlock {
                network: NETWORK,
                previous: base,
                checkpoint: finalized_checkpoint,
                receipts: finalized_receipts.clone(),
            })
            .unwrap();
        let (visited, failures) = run_selective_sweep(
            &payments,
            &receipts,
            finalized_receipts[3].operation_id,
            limit,
        );
        assert_eq!(visited.len(), usize::from(PAYMENT_COUNT));
        assert_eq!(failures, 1);
        payment_intents
    };

    {
        let payments = LmdbPaymentIdempotencyStore::open(
            &payment_path,
            NETWORK,
            PaymentIdempotencyStoreOptions::default(),
        )
        .unwrap();
        let receipts =
            LmdbReceiptIndex::open(&receipt_path, NETWORK, ReceiptIndexStoreOptions::default())
                .unwrap();
        assert_recovered(
            &payments,
            &receipts,
            &payment_intents,
            finalized_checkpoint,
            limit,
        );
    }
    fs::remove_dir_all(payment_path).unwrap();
    fs::remove_dir_all(receipt_path).unwrap();
}
