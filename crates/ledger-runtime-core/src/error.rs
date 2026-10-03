use authenticated_state_core::AuthenticatedStateError;
use ledger_core::LedgerError;
use transaction_protocol::ProtocolError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeError {
    BlockTooLarge,
    StateTooLarge,
    EmptyBlock,
    TooManyOperations,
    EnvelopeTooLarge,
    TooManyEntries,
    InvalidDomain,
    UnsupportedValue,
    InvalidUtf8,
    UnexpectedEnd,
    TrailingBytes,
    LengthOverflow,
    InvalidEnvelope(ProtocolError),
    InvalidOperationId(LedgerError),
    InvalidSnapshot(LedgerError),
    AuthenticatedState(AuthenticatedStateError),
    InvalidStateHeight,
    StateIdentityChanged,
    PreviousStateRootMismatch,
    InvalidProposalLimits,
    InvalidSignatureVerificationPolicy,
    SignatureWorkerFailed,
    NonCanonicalCandidates,
    CandidateOperationMismatch,
    ProposalSizeOverflow,
    ExecutionFailed {
        operation_index: u32,
        source: LedgerError,
    },
}

impl From<AuthenticatedStateError> for RuntimeError {
    fn from(error: AuthenticatedStateError) -> Self {
        Self::AuthenticatedState(error)
    }
}
