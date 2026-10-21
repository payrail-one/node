use ethereum_bridge_core::EvmObserverError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmObserverStoreError {
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongScope,
    CursorMismatch,
    ConflictingBlock,
    ConflictingDeposit,
    InvalidPageLimit,
    CorruptRecord,
}

impl From<EvmObserverError> for EvmObserverStoreError {
    fn from(_error: EvmObserverError) -> Self {
        Self::CorruptRecord
    }
}

impl From<heed::Error> for EvmObserverStoreError {
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

impl From<std::io::Error> for EvmObserverStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}

impl std::fmt::Display for EvmObserverStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EvmObserverStoreError {}
