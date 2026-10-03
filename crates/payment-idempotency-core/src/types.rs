use ledger_core::{AccountId, IdempotencyKey, NetworkId, Nonce, OperationId, OperationReceipt};
use state_sync_core::FinalizedCheckpoint;

pub const MAX_RECONCILIATION_PAGE_SIZE: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct TenantId([u8; 32]);

impl TenantId {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClientRequestKey([u8; 32]);

impl ClientRequestKey {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct RequestDigest([u8; 32]);

impl RequestDigest {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PaymentRequestId {
    pub tenant: TenantId,
    pub account: AccountId,
    pub client_key: ClientRequestKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentIntent {
    pub network: NetworkId,
    pub request_id: PaymentRequestId,
    pub request_digest: RequestDigest,
    pub ledger_idempotency_key: IdempotencyKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSubmission {
    pub nonce: Nonce,
    pub operation_id: OperationId,
    pub envelope: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedPayment {
    pub submission: PreparedSubmission,
    pub checkpoint: FinalizedCheckpoint,
    pub receipt: OperationReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndeterminateReason {
    BroadcastOutcomeUnknown,
    ReconciliationRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectionCode {
    InvalidRequest,
    PolicyDenied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReservationState {
    Reserved,
    NonceAssigned(Nonce),
    SubmissionPrepared(PreparedSubmission),
    Published(PreparedSubmission),
    Indeterminate {
        submission: PreparedSubmission,
        reason: IndeterminateReason,
    },
    Finalized(Box<FinalizedPayment>),
    Rejected {
        code: RejectionCode,
    },
}

impl ReservationState {
    #[must_use]
    pub const fn nonce(&self) -> Option<Nonce> {
        match self {
            Self::NonceAssigned(nonce) => Some(*nonce),
            Self::SubmissionPrepared(submission)
            | Self::Published(submission)
            | Self::Indeterminate { submission, .. } => Some(submission.nonce),
            Self::Finalized(finalized) => Some(finalized.submission.nonce),
            Self::Reserved | Self::Rejected { .. } => None,
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> Option<OperationId> {
        match self {
            Self::SubmissionPrepared(submission)
            | Self::Published(submission)
            | Self::Indeterminate { submission, .. } => Some(submission.operation_id),
            Self::Finalized(finalized) => Some(finalized.submission.operation_id),
            Self::Reserved | Self::NonceAssigned(_) | Self::Rejected { .. } => None,
        }
    }

    #[must_use]
    pub const fn requires_finality_reconciliation(&self) -> bool {
        matches!(
            self,
            Self::SubmissionPrepared(_) | Self::Published(_) | Self::Indeterminate { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconciliationPageLimit(usize);

impl ReconciliationPageLimit {
    /// Creates a bounded page limit suitable for storage adapters.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or a value above the protocol maximum.
    pub const fn new(value: usize) -> Result<Self, crate::PaymentIdempotencyError> {
        if value == 0 || value > MAX_RECONCILIATION_PAGE_SIZE {
            Err(crate::PaymentIdempotencyError::InvalidPageLimit)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationPage {
    pub reservations: Vec<crate::PaymentReservation>,
    pub next_cursor: Option<PaymentRequestId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationOutcome {
    Applied,
    ExistingSame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredMutation {
    pub outcome: MutationOutcome,
    pub reservation: crate::PaymentReservation,
}
