#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentIdempotencyError {
    WrongNetwork,
    RequestConflict,
    InvalidTransition,
    TimestampRegression,
    NonceMismatch,
    OperationMismatch,
    ReceiptMismatch,
    InvalidEnvelope,
    InvalidPageLimit,
    ArithmeticOverflow,
}
