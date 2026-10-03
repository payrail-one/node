use payment_ingress_core::RateLimitError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LmdbRateLimitError {
    RateLimit(RateLimitError),
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    ConfigurationMismatch,
    CorruptRecord,
    IntegerOverflow,
}

impl From<RateLimitError> for LmdbRateLimitError {
    fn from(error: RateLimitError) -> Self {
        Self::RateLimit(error)
    }
}

impl From<heed::Error> for LmdbRateLimitError {
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

impl From<std::io::Error> for LmdbRateLimitError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
