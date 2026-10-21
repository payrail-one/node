use crate::ExternalAssetError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalAssetStateCodecError {
    StateTooLarge,
    TooManyEntries,
    InvalidDomain,
    InvalidValue,
    UnexpectedEnd,
    TrailingBytes,
    Snapshot(ExternalAssetError),
}

impl From<ExternalAssetError> for ExternalAssetStateCodecError {
    fn from(error: ExternalAssetError) -> Self {
        Self::Snapshot(error)
    }
}
