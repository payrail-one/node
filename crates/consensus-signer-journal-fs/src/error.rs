use std::io::ErrorKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileSigningJournalError {
    Io(ErrorKind),
    UnsafeStorePath,
    CorruptRecord,
    TemporaryNameExhausted,
}

impl From<std::io::Error> for FileSigningJournalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}
