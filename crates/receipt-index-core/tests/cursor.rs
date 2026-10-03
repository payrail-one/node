use ledger_core::{
    AccountId, IdempotencyKey, NetworkId, OperationId, OperationKind, OperationReceipt,
};
use receipt_index_core::{
    FinalizedReceiptBlock, ReceiptIndexBase, ReceiptIndexCursor, ReceiptIndexError,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([2; 32]);
const ACCOUNT: AccountId = AccountId::new([3; 32]);

fn checkpoint(height: u64, marker: u8) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height,
        block_hash: BlockHash::new([marker; 32]),
        state_root: StateRoot::new([marker.wrapping_add(1); 32]),
        validator_set_hash: ValidatorSetHash::new([5; 32]),
    }
}

fn receipt(index: u64, nonce: u64, marker: u8) -> OperationReceipt {
    OperationReceipt {
        operation_id: OperationId::new([marker; 32]),
        account: ACCOUNT,
        idempotency_key: IdempotencyKey::new([marker.wrapping_add(1); 32]),
        nonce,
        operation_index: index,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    }
}

fn block(receipts: Vec<OperationReceipt>) -> FinalizedReceiptBlock {
    FinalizedReceiptBlock {
        network: NETWORK,
        previous: checkpoint(100, 10),
        checkpoint: checkpoint(101, 11),
        receipts,
    }
}

#[test]
fn prepared_advance_is_side_effect_free_and_stale_commit_fails() {
    let base = ReceiptIndexBase {
        network: NETWORK,
        checkpoint: checkpoint(100, 10),
        next_operation_index: 7,
    };
    let mut cursor = ReceiptIndexCursor::new(base);
    let prepared = cursor
        .prepare(&block(vec![receipt(7, 0, 20), receipt(8, 1, 21)]))
        .unwrap();

    assert_eq!(cursor.latest(), base.checkpoint);
    assert_eq!(prepared.next_cursor().next_operation_index(), 9);
    cursor.commit(prepared).unwrap();
    assert_eq!(cursor.latest().height, 101);
    assert_eq!(cursor.next_operation_index(), 9);

    assert_eq!(
        cursor.commit(prepared),
        Err(ReceiptIndexError::StalePreparedAdvance)
    );
}

#[test]
fn network_gap_sequence_and_duplicate_fail_without_advancing() {
    let base = ReceiptIndexBase {
        network: NETWORK,
        checkpoint: checkpoint(100, 10),
        next_operation_index: 7,
    };
    let cursor = ReceiptIndexCursor::new(base);

    let mut wrong_network = block(vec![]);
    wrong_network.network = OTHER_NETWORK;
    assert_eq!(
        cursor.prepare(&wrong_network),
        Err(ReceiptIndexError::WrongNetwork)
    );

    let mut gap = block(vec![]);
    gap.checkpoint.height = 102;
    assert_eq!(cursor.prepare(&gap), Err(ReceiptIndexError::CursorMismatch));

    assert_eq!(
        cursor.prepare(&block(vec![receipt(8, 0, 20)])),
        Err(ReceiptIndexError::ReceiptSequenceMismatch {
            expected: 7,
            actual: 8,
        })
    );

    assert_eq!(
        cursor.prepare(&block(vec![receipt(7, 0, 20), receipt(8, 1, 20)])),
        Err(ReceiptIndexError::DuplicateOperation)
    );

    assert_eq!(cursor.latest(), base.checkpoint);
}

#[test]
fn duplicate_account_nonce_is_rejected_even_with_distinct_operation_ids() {
    let cursor = ReceiptIndexCursor::new(ReceiptIndexBase {
        network: NETWORK,
        checkpoint: checkpoint(100, 10),
        next_operation_index: 0,
    });

    assert_eq!(
        cursor.prepare(&block(vec![receipt(0, 4, 20), receipt(1, 4, 21)])),
        Err(ReceiptIndexError::DuplicateAccountNonce)
    );
}
