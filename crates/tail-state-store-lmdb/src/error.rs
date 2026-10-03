#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LmdbStateStoreError {
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    NotInitialized,
    BlockNotFound,
    AlreadyInitialized,
    CursorMismatch,
    ConflictingBlock,
    TreeConflict,
    CommitmentPolicyMismatch,
    VerifiedNetworkConfigRequired,
    NetworkConfigMismatch,
    GenesisCheckpointMismatch,
    CorruptRecord,
    InvalidLedgerState,
}

impl std::fmt::Display for LmdbStateStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for LmdbStateStoreError {}

impl From<heed::Error> for LmdbStateStoreError {
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

impl From<std::io::Error> for LmdbStateStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
