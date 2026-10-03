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
    ClientRequestKey, PaymentIntent, PaymentRequestId, RequestDigest, TenantId,
};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use payment_reconciliation_core::{PaymentFinalityReconciler, PaymentReconciliationOutcome};
use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexBase};
use receipt_index_lmdb::{LmdbReceiptIndex, ReceiptIndexStoreOptions};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const ACCOUNT: AccountId = AccountId::new([2; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-reconciliation-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn checkpoint(height: u64, marker: u8) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height,
        block_hash: BlockHash::new([marker; 32]),
        state_root: StateRoot::new([marker.wrapping_add(1); 32]),
        validator_set_hash: ValidatorSetHash::new([9; 32]),
    }
}

fn intent() -> PaymentIntent {
    PaymentIntent {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([3; 32]),
            account: ACCOUNT,
            client_key: ClientRequestKey::new([4; 32]),
        },
        request_digest: RequestDigest::new([5; 32]),
        ledger_idempotency_key: IdempotencyKey::new([6; 32]),
    }
}

fn signed(payment: PaymentIntent) -> SignedOperation {
    SignedOperation {
        operation: AuthorizedOperation::Transfer(Transfer {
            network: NETWORK,
            idempotency_key: payment.ledger_idempotency_key,
            asset: AssetId::new([7; 32]),
            from: ACCOUNT,
            to: AccountId::new([8; 32]),
            amount: 50,
            fee: 1,
            nonce: 0,
            valid_until_height: 10_000,
        }),
        sender_authorization: Authorization {
            signer: ACCOUNT,
            signature: SignatureBytes::new([10; 64]),
        },
        fee_payer_authorization: None,
    }
}

#[test]
fn finalized_receipt_advances_request_and_survives_restart() {
    let payment_path = directory("payments");
    let receipt_path = directory("receipts");
    let payments = LmdbPaymentIdempotencyStore::open(
        &payment_path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let receipts =
        LmdbReceiptIndex::open(&receipt_path, NETWORK, ReceiptIndexStoreOptions::default())
            .unwrap();
    let base = checkpoint(10, 20);
    receipts
        .initialize(ReceiptIndexBase {
            network: NETWORK,
            checkpoint: base,
            next_operation_index: 0,
        })
        .unwrap();

    let payment = intent();
    let operation = signed(payment);
    let operation_id = operation.operation.operation_id().unwrap();
    payments.reserve(payment, 100).unwrap();
    payments
        .assign_next_nonce(payment.request_id, payment.request_digest, 0, 101)
        .unwrap();
    payments
        .prepare_submission(payment.request_id, payment.request_digest, &operation, 102)
        .unwrap();

    let reconciler = PaymentFinalityReconciler::new(&payments, &receipts);
    assert!(matches!(
        reconciler
            .reconcile(payment.request_id, payment.request_digest, 103)
            .unwrap(),
        PaymentReconciliationOutcome::Pending(_)
    ));
    let finalized_checkpoint = checkpoint(11, 21);
    let receipt = OperationReceipt {
        operation_id,
        account: ACCOUNT,
        idempotency_key: payment.ledger_idempotency_key,
        nonce: 0,
        operation_index: 0,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    };
    receipts
        .commit_finalized(&FinalizedReceiptBlock {
            network: NETWORK,
            previous: base,
            checkpoint: finalized_checkpoint,
            receipts: vec![receipt],
        })
        .unwrap();
    assert!(matches!(
        reconciler
            .reconcile(payment.request_id, payment.request_digest, 104)
            .unwrap(),
        PaymentReconciliationOutcome::Finalized(_)
    ));
    assert!(matches!(
        reconciler
            .reconcile(payment.request_id, payment.request_digest, 105)
            .unwrap(),
        PaymentReconciliationOutcome::AlreadyFinalized(_)
    ));
    drop(receipts);
    drop(payments);

    let reopened = LmdbPaymentIdempotencyStore::open(
        &payment_path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    assert!(matches!(
        reopened
            .get(payment.request_id)
            .unwrap()
            .unwrap()
            .state(),
        payment_idempotency_core::ReservationState::Finalized(finalized)
            if finalized.checkpoint == finalized_checkpoint && finalized.receipt == receipt
    ));
    drop(reopened);
    fs::remove_dir_all(payment_path).unwrap();
    fs::remove_dir_all(receipt_path).unwrap();
}
