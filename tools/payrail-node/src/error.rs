use std::fmt;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub enum NodeError {
    Configuration(&'static str),
    InvalidRequest(&'static str),
    ContractNotFound,
    LocalState,
    UpstreamUnavailable,
    UpstreamRejected(StatusCode, serde_json::Value),
    InvalidUpstream,
    NotSynchronized,
}

impl fmt::Display for NodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Configuration(message) | Self::InvalidRequest(message) => message,
            Self::ContractNotFound => "contract was not found",
            Self::LocalState => "local finalized state is unavailable",
            Self::UpstreamUnavailable => "all configured upstreams are unavailable",
            Self::UpstreamRejected(_, _) => "upstream rejected the signed transaction",
            Self::InvalidUpstream => "upstream returned invalid or conflicting finalized data",
            Self::NotSynchronized => "node has not synchronized to its observed upstream height",
        })
    }
}

impl std::error::Error for NodeError {}

#[derive(Serialize)]
struct ErrorView {
    error: String,
}

impl IntoResponse for NodeError {
    fn into_response(self) -> Response {
        if let Self::UpstreamRejected(status, body) = self {
            return (status, Json(body)).into_response();
        }
        let status = match self {
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::ContractNotFound => StatusCode::NOT_FOUND,
            Self::NotSynchronized => StatusCode::SERVICE_UNAVAILABLE,
            Self::LocalState => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Configuration(_)
            | Self::InvalidUpstream
            | Self::UpstreamUnavailable
            | Self::UpstreamRejected(_, _) => StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(ErrorView {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}
