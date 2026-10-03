#![forbid(unsafe_code)]

mod api;
mod checkout;
mod codec;
mod error;
mod index;
mod live;
mod model;
mod persistence;
mod service;
mod sync;

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    routing::{get, post},
};
use tokio::sync::broadcast;

use crate::model::LiveEvent;

pub use error::DevnetError;
pub use model::{
    AccountStateView, ExplorerOverviewView, NetworkAssetView, NetworkStatusView,
    SubmissionResultView, SubmitRequest, SyncBlockView, SyncBootstrapView, SyncCheckpointView,
};
pub use service::DevnetService;

#[derive(Clone)]
pub struct AppState {
    service: Arc<Mutex<DevnetService>>,
    events: broadcast::Sender<LiveEvent>,
}

impl AppState {
    /// Opens a persistent development network rooted at the supplied directory.
    ///
    /// # Errors
    ///
    /// Returns an error if deterministic genesis construction, persistence
    /// recovery or derived-index reconstruction fails.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, DevnetError> {
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            service: Arc::new(Mutex::new(DevnetService::open(path)?)),
            events,
        })
    }

    fn publish(&self, event: LiveEvent) {
        let _ = self.events.send(event);
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(api::status))
        .route("/api/accounts/{address}", get(api::account))
        .route("/api/faucet", post(api::faucet))
        .route("/api/transactions", post(api::submit))
        .route("/api/explorer", get(api::explorer))
        .route("/api/live", get(live::upgrade))
        .route("/api/sync/bootstrap", get(api::sync_bootstrap))
        .route("/api/sync/blocks/{height}", get(api::sync_block))
        .route("/api/checkouts", post(api::create_checkout))
        .route("/api/checkouts/{id}", get(api::checkout))
        .route(
            "/api/checkouts/{id}/transactions",
            post(api::submit_checkout),
        )
        .with_state(state)
}
