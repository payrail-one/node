#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptIndexError {
    WrongNetwork,
    CursorMismatch,
    TooManyReceipts,
    ReceiptSequenceMismatch { expected: u64, actual: u64 },
    DuplicateOperation,
    DuplicateAccountNonce,
    ArithmeticOverflow,
    StalePreparedAdvance,
}
