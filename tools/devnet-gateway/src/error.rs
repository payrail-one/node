use std::fmt;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::model::ErrorView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevnetError {
    InvalidAddress,
    InvalidHex,
    InvalidEnvelope,
    UnsupportedOperation,
    InvalidTransaction,
    InvalidCheckout,
    CheckoutNotFound,
    CheckoutConflict,
    ApprovalInvalid,
    ApprovalNotFound,
    ApprovalUnauthorized,
    ApprovalRateLimited,
    ApprovalConflict,
    ApprovalUnavailable,
    AccountAlreadyFunded,
    FaucetExhausted,
    SyncBlockNotFound,
    ContractNotFound,
    InvalidSyncBlock,
    InvalidQuorumConfiguration,
    InvalidQuorumRequest,
    ConflictingQuorumVote,
    QuorumUnavailable,
    StateUnavailable,
    InternalInvariant,
}

impl DevnetError {
    const fn status(self) -> StatusCode {
        match self {
            Self::InvalidAddress
            | Self::InvalidHex
            | Self::InvalidEnvelope
            | Self::UnsupportedOperation
            | Self::InvalidTransaction
            | Self::InvalidCheckout
            | Self::InvalidSyncBlock
            | Self::InvalidQuorumRequest
            | Self::ApprovalInvalid => StatusCode::BAD_REQUEST,
            Self::CheckoutNotFound
            | Self::SyncBlockNotFound
            | Self::ApprovalNotFound
            | Self::ContractNotFound => StatusCode::NOT_FOUND,
            Self::ApprovalUnauthorized => StatusCode::UNAUTHORIZED,
            Self::ApprovalRateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::AccountAlreadyFunded
            | Self::CheckoutConflict
            | Self::ConflictingQuorumVote
            | Self::ApprovalConflict => StatusCode::CONFLICT,
            Self::FaucetExhausted | Self::QuorumUnavailable | Self::ApprovalUnavailable => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            Self::StateUnavailable | Self::InternalInvariant | Self::InvalidQuorumConfiguration => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl fmt::Display for DevnetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAddress => "invalid development-network address",
            Self::InvalidHex => "invalid canonical hexadecimal data",
            Self::InvalidEnvelope => "invalid signed transaction envelope",
            Self::UnsupportedOperation => "operation is not enabled by this devnet gateway",
            Self::InvalidTransaction => "transaction was rejected by the development ledger",
            Self::InvalidCheckout => "invalid or expired development checkout",
            Self::CheckoutNotFound => "development checkout was not found",
            Self::CheckoutConflict => "development checkout is already claimed or conflicts",
            Self::ApprovalInvalid => "invalid or expired Payrail Code request",
            Self::ApprovalNotFound => "Payrail Code session was not found",
            Self::ApprovalUnauthorized => "Payrail Code authentication failed",
            Self::ApprovalRateLimited => "Payrail Code request rate limit exceeded",
            Self::ApprovalConflict => "Payrail Code is already linked or conflicts",
            Self::ApprovalUnavailable => "Payrail Code is unavailable on this deployment",
            Self::AccountAlreadyFunded => "this development account already used the faucet",
            Self::FaucetExhausted => "development faucet has insufficient funds",
            Self::SyncBlockNotFound => "finalized synchronization block was not found",
            Self::ContractNotFound => "Payrail contract was not found",
            Self::InvalidSyncBlock => "invalid finalized synchronization block",
            Self::InvalidQuorumConfiguration => "invalid validator quorum configuration",
            Self::InvalidQuorumRequest => "invalid authenticated quorum request",
            Self::ConflictingQuorumVote => "validator refused a conflicting quorum vote",
            Self::QuorumUnavailable => "validator quorum is unavailable",
            Self::StateUnavailable => "development state is unavailable",
            Self::InternalInvariant => "development network invariant failed",
        })
    }
}

impl std::error::Error for DevnetError {}

impl IntoResponse for DevnetError {
    fn into_response(self) -> Response {
        (
            self.status(),
            Json(ErrorView {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}
