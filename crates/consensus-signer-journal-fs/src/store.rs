use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use consensus_signer_core::{Reservation, SigningJournal, VoteIntent, VoteStage};

use crate::{
    FileSigningJournalError,
    codec::{RECORD_LENGTH, RECORD_LENGTH_U64, decode, encode},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const TEMP_ATTEMPTS: usize = 16;

#[derive(Debug)]
pub struct FileSigningJournal {
    root: PathBuf,
}

impl FileSigningJournal {
    /// Opens an application-owned, non-symlink journal directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be safely opened.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, FileSigningJournalError> {
        let requested = root.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(FileSigningJournalError::UnsafeStorePath);
        }
        Ok(Self {
            root: fs::canonicalize(requested)?,
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn reserve_durable(&self, intent: VoteIntent) -> Result<Reservation, FileSigningJournalError> {
        let target = self.root.join(slot_file_name(intent));
        match read_record(&target) {
            Ok(existing) => return Ok(compare(existing, intent)),
            Err(FileSigningJournalError::Io(std::io::ErrorKind::NotFound)) => {}
            Err(error) => return Err(error),
        }

        let (temporary, mut file) = create_temporary(&self.root)?;
        let result = (|| {
            file.write_all(&encode(intent))?;
            file.sync_all()?;
            drop(file);
            match fs::hard_link(&temporary, &target) {
                Ok(()) => {
                    fs::remove_file(&temporary)?;
                    sync_directory(&self.root)?;
                    Ok(Reservation::New)
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&temporary)?;
                    read_record(&target).map(|existing| compare(existing, intent))
                }
                Err(error) => Err(error.into()),
            }
        })();
        if result.is_err() {
            let _ignored = fs::remove_file(&temporary);
        }
        result
    }
}

impl SigningJournal for FileSigningJournal {
    type Error = FileSigningJournalError;

    fn reserve(&mut self, intent: VoteIntent) -> Result<Reservation, Self::Error> {
        self.reserve_durable(intent)
    }
}

fn compare(existing: VoteIntent, requested: VoteIntent) -> Reservation {
    if existing == requested {
        Reservation::ExistingSame
    } else {
        Reservation::Conflict
    }
}

fn read_record(path: &Path) -> Result<VoteIntent, FileSigningJournalError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() != RECORD_LENGTH_U64 {
        return Err(FileSigningJournalError::CorruptRecord);
    }
    let mut bytes = Vec::with_capacity(RECORD_LENGTH);
    File::open(path)?.read_to_end(&mut bytes)?;
    decode(&bytes)
}

fn create_temporary(root: &Path) -> Result<(PathBuf, File), FileSigningJournalError> {
    for _ in 0..TEMP_ATTEMPTS {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            ".signing-journal-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(FileSigningJournalError::TemporaryNameExhausted)
}

fn slot_file_name(intent: VoteIntent) -> String {
    let stage = match intent.slot.stage {
        VoteStage::Proposal => "proposal",
        VoteStage::Prevote => "prevote",
        VoteStage::Precommit => "precommit",
    };
    format!(
        "{}-{:016x}-{:016x}-{:016x}-{stage}.vote",
        hex(intent.slot.network.as_bytes()),
        intent.slot.set_id,
        intent.slot.height,
        intent.slot.round
    )
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn sync_directory(directory: &Path) -> Result<(), FileSigningJournalError> {
    File::open(directory)?.sync_all()?;
    Ok(())
}
