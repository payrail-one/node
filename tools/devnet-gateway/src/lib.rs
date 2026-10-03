#![forbid(unsafe_code)]

mod api;
mod checkout;
mod codec;
mod error;
mod index;
mod model;
mod persistence;
mod service;

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    routing::{get, post},
};

pub use error::DevnetError;
pub use service::DevnetService;

#[derive(Clone)]
pub struct AppState {
    service: Arc<Mutex<DevnetService>>,
}

impl AppState {
    /// Opens a persistent development network rooted at the supplied directory.
    ///
    /// # Errors
    ///
    /// Returns an error if deterministic genesis construction, persistence
    /// recovery or derived-index reconstruction fails.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, DevnetError> {
        Ok(Self {
            service: Arc::new(Mutex::new(DevnetService::open(path)?)),
        })
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(api::status))
        .route("/api/accounts/{address}", get(api::account))
        .route("/api/faucet", post(api::faucet))
        .route("/api/transactions", post(api::submit))
        .route("/api/explorer", get(api::explorer))
        .route("/api/checkouts", post(api::create_checkout))
        .route("/api/checkouts/{id}", get(api::checkout))
        .route(
            "/api/checkouts/{id}/transactions",
            post(api::submit_checkout),
        )
        .with_state(state)
}
