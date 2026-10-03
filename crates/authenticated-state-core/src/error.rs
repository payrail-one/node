#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticatedStateError {
    InvalidVersion,
    TooManyChanges,
    EmptyKey,
    KeyTooLarge,
    ValueTooLarge,
    DuplicateKey,
    BackendFailure,
    StoreConflict,
}
