use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use checkout_approval_auth_hmac::HmacApprovalCodeAuthenticator;
use checkout_approval_core::{
    ApprovalCode, ApprovalCodeAuthenticator, ApprovalCodeError, ApprovalCodeMutationOutcome,
    ApprovalCodePolicy, ApprovalCodeRecord, ApprovalCodeState,
};
use checkout_approval_lmdb::{
    ApprovalCodeStoreError, ApprovalCodeStoreOptions, LmdbApprovalCodeStore,
};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_directory(label: &str) -> PathBuf {
    let sequence = DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "approval-code-{label}-{}-{sequence}",
        std::process::id()
    ))
}

fn network() -> NetworkId {
    NetworkId::new([9; 32])
}

fn authenticator() -> HmacApprovalCodeAuthenticator {
    HmacApprovalCodeAuthenticator::new(&[17; 32]).unwrap()
}

fn approval(code: &str, account: AccountId, issued_at_ms: u64) -> ApprovalCodeRecord {
    let code = ApprovalCode::parse(code).unwrap();
    ApprovalCodeRecord::issue(
        network(),
        authenticator().digest(network(), code),
        account,
        ApprovalCodePolicy::TWO_MINUTES,
        issued_at_ms,
    )
    .unwrap()
}

#[test]
fn claim_consume_and_restart_preserve_atomic_indexes() {
    let path = temporary_directory("lifecycle");
    let account = AccountId::new([1; 32]);
    let checkout = CheckoutId::new([2; 32]);
    let record = approval("123456", account, 1_000);
    let store =
        LmdbApprovalCodeStore::open(&path, network(), ApprovalCodeStoreOptions::default()).unwrap();

    assert_eq!(
        store.issue(record, 1_000).unwrap().outcome,
        ApprovalCodeMutationOutcome::Applied
    );
    assert_eq!(store.by_digest(record.digest()).unwrap(), Some(record));
    let claimed = store.claim(record.digest(), checkout, 2_000).unwrap();
    assert_eq!(claimed.outcome, ApprovalCodeMutationOutcome::Applied);
    assert!(matches!(
        claimed.record.state(),
        ApprovalCodeState::Claimed(_)
    ));
    assert_eq!(
        store
            .claim(record.digest(), checkout, 2_000)
            .unwrap()
            .outcome,
        ApprovalCodeMutationOutcome::ExistingSame
    );
    assert_eq!(
        store.consume(checkout, AccountId::new([8; 32]), 3_000),
        Err(ApprovalCodeStoreError::Domain(
            ApprovalCodeError::AccountMismatch
        ))
    );
    let consumed = store.consume(checkout, account, 3_000).unwrap();
    assert!(matches!(
        consumed.record.state(),
        ApprovalCodeState::Consumed { .. }
    ));
    drop(store);

    let reopened =
        LmdbApprovalCodeStore::open(&path, network(), ApprovalCodeStoreOptions::default()).unwrap();
    assert_eq!(
        reopened.by_checkout(checkout).unwrap(),
        Some(consumed.record)
    );
    assert_eq!(
        reopened.by_digest(record.digest()).unwrap(),
        Some(consumed.record)
    );
    assert_eq!(
        reopened.consume(checkout, account, 3_000).unwrap().outcome,
        ApprovalCodeMutationOutcome::ExistingSame
    );
    drop(reopened);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn ready_code_rotates_but_live_claim_cannot_be_replaced() {
    let path = temporary_directory("rotation");
    let account = AccountId::new([3; 32]);
    let first = approval("111111", account, 10_000);
    let second = approval("222222", account, 11_000);
    let third = approval("333333", account, 12_000);
    let store =
        LmdbApprovalCodeStore::open(&path, network(), ApprovalCodeStoreOptions::default()).unwrap();

    store.issue(first, 10_000).unwrap();
    store.issue(second, 11_000).unwrap();
    assert_eq!(
        store.claim(first.digest(), CheckoutId::new([4; 32]), 11_001),
        Err(ApprovalCodeStoreError::CodeNotFound)
    );
    store
        .claim(second.digest(), CheckoutId::new([5; 32]), 11_001)
        .unwrap();
    assert_eq!(
        store.issue(third, 12_000),
        Err(ApprovalCodeStoreError::Domain(
            ApprovalCodeError::AccountHasClaimedCode
        ))
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn digest_and_checkout_collisions_are_rejected() {
    let path = temporary_directory("collision");
    let checkout = CheckoutId::new([6; 32]);
    let first = approval("444444", AccountId::new([1; 32]), 10_000);
    let same_digest = approval("444444", AccountId::new([2; 32]), 10_001);
    let second = approval("555555", AccountId::new([2; 32]), 10_001);
    let store =
        LmdbApprovalCodeStore::open(&path, network(), ApprovalCodeStoreOptions::default()).unwrap();

    store.issue(first, 10_000).unwrap();
    assert_eq!(
        store.issue(same_digest, 10_001),
        Err(ApprovalCodeStoreError::CodeCollision)
    );
    store.issue(second, 10_001).unwrap();
    store.claim(first.digest(), checkout, 10_002).unwrap();
    assert_eq!(
        store.claim(second.digest(), checkout, 10_002),
        Err(ApprovalCodeStoreError::Domain(
            ApprovalCodeError::CheckoutAlreadyLinked
        ))
    );
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn database_is_bound_to_one_network() {
    let path = temporary_directory("network");
    let store =
        LmdbApprovalCodeStore::open(&path, network(), ApprovalCodeStoreOptions::default()).unwrap();
    drop(store);
    assert!(matches!(
        LmdbApprovalCodeStore::open(
            &path,
            NetworkId::new([10; 32]),
            ApprovalCodeStoreOptions::default()
        ),
        Err(ApprovalCodeStoreError::WrongNetwork)
    ));
    fs::remove_dir_all(path).unwrap();
}
