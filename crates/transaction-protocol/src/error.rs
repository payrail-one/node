#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    EnvelopeTooLarge,
    InvalidDomain,
    UnsupportedOperation,
    InvalidAuthorizationFlag,
    AuthorizationSetMismatch,
    InvalidOperation,
    BatchTooLarge,
    UnexpectedEnd,
    TrailingBytes,
}
