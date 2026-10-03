#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TailSyncError {
    WrongNetwork,
    HeightOverflow,
    NonSequentialHeight,
    ParentHashMismatch,
    PayloadTooLarge,
    FinalityProofTooLarge,
    InvalidFinalityProof,
    InvalidTransition,
    StaleVerifiedBlock,
    InvalidDomain,
    UnexpectedEnd,
    TrailingBytes,
}
