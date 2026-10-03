#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCodeError {
    InvalidCode,
    InvalidPolicy,
    InvalidExpiry,
    Expired,
    AlreadyClaimed,
    AccountHasClaimedCode,
    AccountMismatch,
    CheckoutAlreadyLinked,
    InvalidTransition,
    TimestampRegression,
    CorruptState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApprovalCodeServiceError<StoreError> {
    Domain(ApprovalCodeError),
    Store(StoreError),
    CodeCollision,
    CodeNotFound,
}

impl<StoreError> From<ApprovalCodeError> for ApprovalCodeServiceError<StoreError> {
    fn from(error: ApprovalCodeError) -> Self {
        Self::Domain(error)
    }
}
