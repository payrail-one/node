use receipt_index_core::ReceiptIndexError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptIndexStoreError {
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    NotInitialized,
    AlreadyInitialized,
    CursorMismatch,
    ConflictingBlock,
    DuplicateOperation,
    DuplicateAccountNonce,
    CorruptRecord,
}

impl From<ReceiptIndexError> for ReceiptIndexStoreError {
    fn from(error: ReceiptIndexError) -> Self {
        match error {
            ReceiptIndexError::WrongNetwork => Self::WrongNetwork,
            ReceiptIndexError::CursorMismatch | ReceiptIndexError::StalePreparedAdvance => {
                Self::CursorMismatch
            }
            ReceiptIndexError::DuplicateOperation => Self::DuplicateOperation,
            ReceiptIndexError::DuplicateAccountNonce => Self::DuplicateAccountNonce,
            ReceiptIndexError::TooManyReceipts
            | ReceiptIndexError::ReceiptSequenceMismatch { .. }
            | ReceiptIndexError::ArithmeticOverflow => Self::CorruptRecord,
        }
    }
}

impl From<heed::Error> for ReceiptIndexStoreError {
    fn from(error: heed::Error) -> Self {
        match error {
            heed::Error::Mdb(heed::MdbError::MapFull) => Self::MapFull,
            heed::Error::Mdb(
                heed::MdbError::PageNotFound
                | heed::MdbError::Corrupted
                | heed::MdbError::Panic
                | heed::MdbError::VersionMismatch
                | heed::MdbError::Invalid,
            ) => Self::CorruptRecord,
            _ => Self::Database,
        }
    }
}

impl From<std::io::Error> for ReceiptIndexStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
