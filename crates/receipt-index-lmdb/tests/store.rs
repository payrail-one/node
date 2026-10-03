use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, IdempotencyKey, NetworkId, OperationId, OperationKind, OperationReceipt,
};
use receipt_index_core::{FinalizedReceiptBlock, ReceiptIndexBase};
use receipt_index_lmdb::{
    LmdbReceiptIndex, ReceiptIndexCommitOutcome, ReceiptIndexStoreError, ReceiptIndexStoreOptions,
};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([2; 32]);
const ACCOUNT: AccountId = AccountId::new([3; 32]);
const SHARED_KEY: IdempotencyKey = IdempotencyKey::new([4; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "receipt-index-lmdb-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn checkpoint(height: u64, marker: u8) -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height,
        block_hash: BlockHash::new([marker; 32]),
        state_root: StateRoot::new([marker.wrapping_add(1); 32]),
        validator_set_hash: ValidatorSetHash::new([5; 32]),
    }
}

fn base() -> ReceiptIndexBase {
    ReceiptIndexBase {
        network: NETWORK,
        checkpoint: checkpoint(100, 10),
        next_operation_index: 7,
    }
}

fn receipt(index: u64, nonce: u64, marker: u8, key: IdempotencyKey) -> OperationReceipt {
    OperationReceipt {
        operation_id: OperationId::new([marker; 32]),
        account: ACCOUNT,
        idempotency_key: key,
        nonce,
        operation_index: index,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    }
}

fn first_block() -> FinalizedReceiptBlock {
    FinalizedReceiptBlock {
        network: NETWORK,
        previous: base().checkpoint,
        checkpoint: checkpoint(101, 11),
        receipts: vec![
            receipt(7, 0, 20, SHARED_KEY),
            receipt(8, 1, 21, IdempotencyKey::new([22; 32])),
        ],
    }
}

fn second_block() -> FinalizedReceiptBlock {
    let mut expired = receipt(9, 2, 23, SHARED_KEY);
    expired.outcome = ledger_core::OperationOutcome::Expired;
    FinalizedReceiptBlock {
        network: NETWORK,
        previous: first_block().checkpoint,
        checkpoint: checkpoint(102, 12),
        receipts: vec![expired],
    }
}

#[test]
fn atomic_index_survives_restart_and_supports_all_lookup_keys() {
    let path = directory("restart");
    let first = first_block();
    let second = second_block();
    {
        let store =
            LmdbReceiptIndex::open(&path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
        assert_eq!(store.cursor(), Err(ReceiptIndexStoreError::NotInitialized));
        store.initialize(base()).unwrap();
        assert_eq!(
            store.commit_finalized(&first).unwrap(),
            ReceiptIndexCommitOutcome::Committed
        );
        assert_eq!(
            store.commit_finalized(&first).unwrap(),
            ReceiptIndexCommitOutcome::ExistingSame
        );
        assert_eq!(
            store.commit_finalized(&second).unwrap(),
            ReceiptIndexCommitOutcome::Committed
        );
        let indexed = store.by_operation_index(9).unwrap().unwrap();
        assert_eq!(indexed.receipt, second.receipts[0]);
        assert_eq!(indexed.checkpoint, second.checkpoint);
        assert_eq!(indexed.operation_offset, 0);
        assert_eq!(
            store.by_account_nonce(ACCOUNT, 2).unwrap().unwrap(),
            indexed
        );
        assert_eq!(
            store
                .by_correlation(ACCOUNT, SHARED_KEY, 2)
                .unwrap()
                .unwrap(),
            indexed
        );
        assert_ne!(
            store
                .by_correlation(ACCOUNT, SHARED_KEY, 0)
                .unwrap()
                .unwrap(),
            indexed
        );
    }

    let reopened =
        LmdbReceiptIndex::open(&path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
    assert_eq!(reopened.cursor().unwrap().latest(), second.checkpoint);
    assert_eq!(reopened.cursor().unwrap().next_operation_index(), 10);
    assert_eq!(
        reopened
            .by_operation_id(second.receipts[0].operation_id)
            .unwrap()
            .unwrap()
            .receipt,
        second.receipts[0]
    );
    assert_eq!(reopened.by_operation_index(6).unwrap(), None);
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn rejected_gap_sequence_and_conflicting_retry_do_not_advance() {
    let path = directory("rejections");
    let store =
        LmdbReceiptIndex::open(&path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
    store.initialize(base()).unwrap();
    let mut gap = first_block();
    gap.checkpoint.height = 102;
    assert_eq!(
        store.commit_finalized(&gap),
        Err(ReceiptIndexStoreError::CursorMismatch)
    );
    let mut bad_sequence = first_block();
    bad_sequence.receipts[0].operation_index = 8;
    assert_eq!(
        store.commit_finalized(&bad_sequence),
        Err(ReceiptIndexStoreError::CorruptRecord)
    );
    assert_eq!(store.cursor().unwrap().latest(), base().checkpoint);

    let first = first_block();
    store.commit_finalized(&first).unwrap();
    let mut conflict = first.clone();
    conflict.receipts[0].operation_id = OperationId::new([99; 32]);
    assert_eq!(
        store.commit_finalized(&conflict),
        Err(ReceiptIndexStoreError::ConflictingBlock)
    );
    assert_eq!(store.cursor().unwrap().latest(), first.checkpoint);
    assert_eq!(
        store.by_operation_id(OperationId::new([99; 32])).unwrap(),
        None
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn historical_operation_and_account_nonce_collisions_fail_atomically() {
    let path = directory("collisions");
    let store =
        LmdbReceiptIndex::open(&path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
    store.initialize(base()).unwrap();
    let first = first_block();
    store.commit_finalized(&first).unwrap();

    let mut duplicate_operation = second_block();
    duplicate_operation.receipts[0].operation_id = first.receipts[0].operation_id;
    assert_eq!(
        store.commit_finalized(&duplicate_operation),
        Err(ReceiptIndexStoreError::DuplicateOperation)
    );

    let mut duplicate_nonce = second_block();
    duplicate_nonce.receipts[0].nonce = first.receipts[0].nonce;
    assert_eq!(
        store.commit_finalized(&duplicate_nonce),
        Err(ReceiptIndexStoreError::DuplicateAccountNonce)
    );
    assert_eq!(store.cursor().unwrap().latest(), first.checkpoint);
    assert_eq!(store.by_operation_index(9).unwrap(), None);
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn network_binding_and_initialization_fail_closed() {
    let path = directory("network");
    {
        let store =
            LmdbReceiptIndex::open(&path, NETWORK, ReceiptIndexStoreOptions::default()).unwrap();
        let mut wrong = base();
        wrong.network = OTHER_NETWORK;
        assert_eq!(
            store.initialize(wrong),
            Err(ReceiptIndexStoreError::WrongNetwork)
        );
        store.initialize(base()).unwrap();
        assert_eq!(
            store.initialize(base()),
            Err(ReceiptIndexStoreError::AlreadyInitialized)
        );
    }
    assert_eq!(
        LmdbReceiptIndex::open(&path, OTHER_NETWORK, ReceiptIndexStoreOptions::default())
            .unwrap_err(),
        ReceiptIndexStoreError::WrongNetwork
    );
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn small_map_is_rejected_before_open() {
    let path = directory("small-map");
    assert_eq!(
        LmdbReceiptIndex::open(
            &path,
            NETWORK,
            ReceiptIndexStoreOptions {
                map_size: 1024,
                max_readers: 4,
            },
        )
        .unwrap_err(),
        ReceiptIndexStoreError::MapSizeTooSmall
    );
    assert!(!path.exists());
}

#[test]
fn map_full_keeps_receipts_block_and_cursor_atomic() {
    let path = directory("map-full");
    let store = LmdbReceiptIndex::open(
        &path,
        NETWORK,
        ReceiptIndexStoreOptions {
            map_size: 16 * 1024 * 1024,
            max_readers: 4,
        },
    )
    .unwrap();
    store.initialize(base()).unwrap();
    let mut previous = base().checkpoint;
    let mut next_operation_index = base().next_operation_index;
    let mut failed_operation = None;

    for block_number in 1_u64..=32 {
        let receipts = (0_u64..4_096)
            .map(|offset| bulk_receipt(next_operation_index + offset))
            .collect::<Vec<_>>();
        let first_operation = receipts[0].operation_id;
        let block = FinalizedReceiptBlock {
            network: NETWORK,
            previous,
            checkpoint: checkpoint(
                previous.height + 1,
                u8::try_from(block_number).unwrap().wrapping_add(100),
            ),
            receipts,
        };
        match store.commit_finalized(&block) {
            Ok(ReceiptIndexCommitOutcome::Committed) => {
                previous = block.checkpoint;
                next_operation_index += 4_096;
            }
            Err(ReceiptIndexStoreError::MapFull) => {
                failed_operation = Some(first_operation);
                break;
            }
            result => panic!("unexpected receipt-index result: {result:?}"),
        }
    }

    let failed_operation = failed_operation.expect("test must exhaust the fixed LMDB map");
    assert_eq!(store.cursor().unwrap().latest(), previous);
    assert_eq!(
        store.cursor().unwrap().next_operation_index(),
        next_operation_index
    );
    assert_eq!(store.by_operation_id(failed_operation).unwrap(), None);
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

fn bulk_receipt(operation_index: u64) -> OperationReceipt {
    let mut operation_id = [50; 32];
    operation_id[24..].copy_from_slice(&operation_index.to_be_bytes());
    let mut idempotency_key = [51; 32];
    idempotency_key[24..].copy_from_slice(&operation_index.to_be_bytes());
    OperationReceipt {
        operation_id: OperationId::new(operation_id),
        account: ACCOUNT,
        idempotency_key: IdempotencyKey::new(idempotency_key),
        nonce: operation_index,
        operation_index,
        kind: OperationKind::Transfer,
        outcome: ledger_core::OperationOutcome::Applied,
    }
}
