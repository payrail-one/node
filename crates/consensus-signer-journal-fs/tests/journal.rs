use std::{fs, sync::Barrier, thread};

use consensus_signer_core::{Reservation, SigningJournal, VoteIntent, VoteSlot, VoteStage};
use consensus_signer_journal_fs::{FileSigningJournal, FileSigningJournalError};
use ledger_core::NetworkId;
use state_sync_core::BlockHash;

fn temporary_directory(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "consensus-journal-{name}-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
    let _ignored = fs::remove_dir_all(&path);
    path
}

fn intent(hash: u8) -> VoteIntent {
    VoteIntent {
        slot: VoteSlot {
            network: NetworkId::new([1; 32]),
            set_id: 5,
            height: 100,
            round: 9,
            stage: VoteStage::Precommit,
        },
        target_hash: BlockHash::new([hash; 32]),
    }
}

#[test]
fn reservation_survives_restart_and_rejects_conflict() {
    let path = temporary_directory("restart");
    let mut journal = FileSigningJournal::open(&path).unwrap();
    assert_eq!(journal.reserve(intent(2)).unwrap(), Reservation::New);
    drop(journal);

    let mut reopened = FileSigningJournal::open(&path).unwrap();
    assert_eq!(
        reopened.reserve(intent(2)).unwrap(),
        Reservation::ExistingSame
    );
    assert_eq!(reopened.reserve(intent(3)).unwrap(), Reservation::Conflict);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn next_height_uses_an_independent_durable_slot() {
    let path = temporary_directory("next-height");
    let mut journal = FileSigningJournal::open(&path).unwrap();
    let first = intent(2);
    let mut next = intent(3);
    next.slot.height = first.slot.height + 1;

    assert_eq!(journal.reserve(first).unwrap(), Reservation::New);
    assert_eq!(journal.reserve(next).unwrap(), Reservation::New);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn concurrent_writers_publish_only_one_target() {
    let path = temporary_directory("race");
    fs::create_dir_all(&path).unwrap();
    let barrier = std::sync::Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for hash in [2, 3] {
        let worker_path = path.clone();
        let worker_barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            let mut journal = FileSigningJournal::open(worker_path).unwrap();
            worker_barrier.wait();
            journal.reserve(intent(hash)).unwrap()
        }));
    }
    barrier.wait();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(results.contains(&Reservation::New));
    assert!(results.contains(&Reservation::Conflict));
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn corrupt_existing_record_fails_closed() {
    let path = temporary_directory("corrupt");
    let mut journal = FileSigningJournal::open(&path).unwrap();
    assert_eq!(journal.reserve(intent(2)).unwrap(), Reservation::New);
    let record = fs::read_dir(&path)
        .unwrap()
        .find_map(|entry| {
            let entry = entry.unwrap();
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "vote")
                .then(|| entry.path())
        })
        .unwrap();
    fs::write(record, b"broken").unwrap();
    assert_eq!(
        journal.reserve(intent(2)),
        Err(FileSigningJournalError::CorruptRecord)
    );
    fs::remove_dir_all(path).unwrap();
}
