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
    AccountAlreadyFunded,
    FaucetExhausted,
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
            | Self::InvalidCheckout => StatusCode::BAD_REQUEST,
            Self::CheckoutNotFound => StatusCode::NOT_FOUND,
            Self::AccountAlreadyFunded | Self::CheckoutConflict => StatusCode::CONFLICT,
            Self::FaucetExhausted => StatusCode::SERVICE_UNAVAILABLE,
            Self::StateUnavailable | Self::InternalInvariant => StatusCode::INTERNAL_SERVER_ERROR,
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
            Self::AccountAlreadyFunded => "this development account already used the faucet",
            Self::FaucetExhausted => "development faucet has insufficient funds",
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
