use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmChainId, EvmFinalitySource,
    EvmFinalizedBlock, EvmHash, EvmLog, EvmObserverScope, FinalizedEvmObserverBlock,
    MAX_DEPOSITS_PER_BLOCK, prepare_observer_block, verify_finalized_deposit,
};
use ethereum_bridge_observer_lmdb::{
    EvmObserverCommitOutcome, EvmObserverStoreError, EvmObserverStoreOptions, LmdbEvmObserverStore,
    MAX_DEPOSIT_PAGE_SIZE, MINIMUM_MAP_SIZE,
};
use external_asset_core::DepositId;
use ledger_core::{AccountId, AssetId};

const BRIDGE: EvmAddress = EvmAddress::new([1; 20]);
const TOKEN: EvmAddress = EvmAddress::new([2; 20]);
const PARENT: EvmHash = EvmHash::new([3; 32]);
const BLOCK: EvmHash = EvmHash::new([4; 32]);
const ASSET: AssetId = AssetId::new([5; 32]);
const RECIPIENT: AccountId = AccountId::new([6; 32]);
const FIRST_BLOCK: u64 = 100;
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ethereum-observer-lmdb-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn chain_id() -> EvmChainId {
    EvmChainId::new(1).unwrap()
}

fn scope() -> EvmObserverScope {
    EvmObserverScope::new(chain_id(), BRIDGE, FIRST_BLOCK, PARENT).unwrap()
}

fn binding() -> EvmAssetBinding {
    EvmAssetBinding::new(chain_id(), BRIDGE, EvmAsset::Erc20(TOKEN), ASSET).unwrap()
}

fn finalized(number: u64, hash: EvmHash, proof: u8) -> EvmFinalizedBlock {
    EvmFinalizedBlock {
        number,
        hash,
        proof_hash: [proof; 32],
    }
}

struct Source(EvmFinalizedBlock);

impl EvmFinalitySource for Source {
    type Error = ();

    fn finalized_canonical_block(
        &self,
        _chain_id: EvmChainId,
        _number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error> {
        Ok(Some(self.0))
    }
}

fn log(number: u64, hash: EvmHash, index: u32) -> EvmLog {
    let mut token_topic = [0_u8; 32];
    token_topic[12..].copy_from_slice(TOKEN.as_bytes());
    let mut transaction = [0_u8; 32];
    transaction[28..].copy_from_slice(&index.saturating_add(1).to_be_bytes());
    let mut data = vec![0_u8; 64];
    data[31] = 1;
    data[60..].copy_from_slice(&index.to_be_bytes());
    EvmLog {
        address: BRIDGE,
        topics: vec![
            DEPOSITED_EVENT_TOPIC,
            EvmHash::new(*RECIPIENT.as_bytes()),
            EvmHash::new(token_topic),
        ],
        data,
        transaction_hash: Some(EvmHash::new(transaction)),
        block_hash: Some(hash),
        block_number: Some(number),
        log_index: Some(u64::from(index)),
        removed: false,
    }
}

fn deposit(
    number: u64,
    hash: EvmHash,
    proof: u8,
    index: u32,
) -> ethereum_bridge_core::VerifiedEvmDeposit {
    verify_finalized_deposit(
        binding(),
        &log(number, hash, index),
        &Source(finalized(number, hash, proof)),
    )
    .unwrap()
}

fn prepared_first(count: u32) -> ethereum_bridge_core::PreparedEvmObserverBlock {
    let observer_scope = scope();
    let deposits = (0..count)
        .map(|index| deposit(FIRST_BLOCK, BLOCK, 7, index))
        .collect();
    prepare_observer_block(
        observer_scope,
        observer_scope.initial_checkpoint(),
        FinalizedEvmObserverBlock {
            finalized: finalized(FIRST_BLOCK, BLOCK, 7),
            parent_hash: PARENT,
            deposits,
        },
    )
    .unwrap()
}

#[test]
fn atomic_block_survives_restart_and_exact_retry() {
    let path = directory("restart");
    let prepared = prepared_first(2);
    let ids: Vec<_> = prepared
        .deposits()
        .iter()
        .map(|deposit| DepositId::derive(&deposit.claim()))
        .collect();
    {
        let store =
            LmdbEvmObserverStore::open(&path, scope(), EvmObserverStoreOptions::default()).unwrap();
        assert_eq!(store.checkpoint().unwrap(), scope().initial_checkpoint());
        assert_eq!(
            store.commit(&prepared),
            Ok(EvmObserverCommitOutcome::Committed)
        );
        assert_eq!(
            store.commit(&prepared),
            Ok(EvmObserverCommitOutcome::ExistingSame)
        );
        assert_eq!(store.checkpoint().unwrap(), prepared.next());
        let first = store.deposit(ids[0]).unwrap().unwrap();
        assert_eq!(first.claim, prepared.deposits()[0].claim());
        assert_eq!(first.evidence, prepared.deposits()[0].evidence());
    }

    let reopened =
        LmdbEvmObserverStore::open(&path, scope(), EvmObserverStoreOptions::default()).unwrap();
    assert_eq!(reopened.checkpoint().unwrap(), prepared.next());
    assert!(reopened.deposit(ids[1]).unwrap().is_some());
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn conflicting_retry_and_wrong_scope_leave_state_unchanged() {
    let path = directory("conflict");
    let store =
        LmdbEvmObserverStore::open(&path, scope(), EvmObserverStoreOptions::default()).unwrap();
    let first = prepared_first(1);
    store.commit(&first).unwrap();

    let alternate_hash = EvmHash::new([99; 32]);
    let alternate = prepare_observer_block(
        scope(),
        scope().initial_checkpoint(),
        FinalizedEvmObserverBlock {
            finalized: finalized(FIRST_BLOCK, alternate_hash, 8),
            parent_hash: PARENT,
            deposits: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(
        store.commit(&alternate),
        Err(EvmObserverStoreError::ConflictingBlock)
    );

    let other_scope =
        EvmObserverScope::new(chain_id(), EvmAddress::new([77; 20]), FIRST_BLOCK, PARENT).unwrap();
    let other = prepare_observer_block(
        other_scope,
        other_scope.initial_checkpoint(),
        FinalizedEvmObserverBlock {
            finalized: finalized(FIRST_BLOCK, BLOCK, 7),
            parent_hash: PARENT,
            deposits: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(store.commit(&other), Err(EvmObserverStoreError::WrongScope));
    assert_eq!(store.checkpoint().unwrap(), first.next());
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn scope_binding_and_options_are_enforced_on_reopen() {
    let path = directory("scope");
    let store =
        LmdbEvmObserverStore::open(&path, scope(), EvmObserverStoreOptions::default()).unwrap();
    drop(store);

    let other_scope =
        EvmObserverScope::new(chain_id(), EvmAddress::new([88; 20]), FIRST_BLOCK, PARENT).unwrap();
    assert_eq!(
        LmdbEvmObserverStore::open(&path, other_scope, EvmObserverStoreOptions::default())
            .unwrap_err(),
        EvmObserverStoreError::WrongScope
    );
    assert_eq!(
        LmdbEvmObserverStore::open(
            directory("small-map"),
            scope(),
            EvmObserverStoreOptions {
                map_size: MINIMUM_MAP_SIZE - 1,
                max_readers: 1,
            }
        )
        .unwrap_err(),
        EvmObserverStoreError::MapSizeTooSmall
    );
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn deterministic_pages_are_bounded_and_resume_after_id() {
    let path = directory("pages");
    let store =
        LmdbEvmObserverStore::open(&path, scope(), EvmObserverStoreOptions::default()).unwrap();
    store.commit(&prepared_first(3)).unwrap();

    let all = store.deposits_after(None, 10).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.windows(2).all(|pair| pair[0].id < pair[1].id));
    let remainder = store.deposits_after(Some(all[0].id), 10).unwrap();
    assert_eq!(remainder, all[1..]);
    assert_eq!(
        store.deposits_after(None, 0),
        Err(EvmObserverStoreError::InvalidPageLimit)
    );
    assert_eq!(
        store.deposits_after(None, MAX_DEPOSIT_PAGE_SIZE + 1),
        Err(EvmObserverStoreError::InvalidPageLimit)
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn map_full_aborts_deposits_and_cursor_together() {
    let path = directory("map-full");
    let store = LmdbEvmObserverStore::open(
        &path,
        scope(),
        EvmObserverStoreOptions {
            map_size: MINIMUM_MAP_SIZE,
            max_readers: 8,
        },
    )
    .unwrap();
    let oversized_for_map = prepared_first(u32::try_from(MAX_DEPOSITS_PER_BLOCK).unwrap());
    assert_eq!(
        store.commit(&oversized_for_map),
        Err(EvmObserverStoreError::MapFull)
    );
    assert_eq!(store.checkpoint().unwrap(), scope().initial_checkpoint());
    assert!(store.deposits_after(None, 1).unwrap().is_empty());
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_store_path_is_rejected() {
    use std::os::unix::fs::symlink;

    let target = directory("symlink-target");
    let link = directory("symlink-link");
    fs::create_dir_all(&target).unwrap();
    symlink(&target, &link).unwrap();
    assert_eq!(
        LmdbEvmObserverStore::open(&link, scope(), EvmObserverStoreOptions::default()).unwrap_err(),
        EvmObserverStoreError::UnsafePath
    );
    fs::remove_file(link).unwrap();
    fs::remove_dir_all(target).unwrap();
}
