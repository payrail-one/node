use checkout_approval_core::ApprovalCodeError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCodeStoreError {
    Domain(ApprovalCodeError),
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    CodeCollision,
    CodeNotFound,
    CorruptRecord,
}

impl From<ApprovalCodeError> for ApprovalCodeStoreError {
    fn from(error: ApprovalCodeError) -> Self {
        Self::Domain(error)
    }
}

impl From<heed::Error> for ApprovalCodeStoreError {
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

impl From<std::io::Error> for ApprovalCodeStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
