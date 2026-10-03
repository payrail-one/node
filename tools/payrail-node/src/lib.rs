#![forbid(unsafe_code)]

mod api;
mod config;
mod error;
mod state;
mod upstream;

use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

pub use config::NodeConfig;
pub use error::NodeError;
pub use state::NodeState;
pub use upstream::UpstreamClient;

pub fn router(state: NodeState) -> Router {
    Router::new()
        .route("/health/live", get(api::live))
        .route("/health/ready", get(api::ready))
        .route("/api/status", get(api::status))
        .route("/api/accounts/{address}", get(api::account))
        .route("/api/transactions", post(api::submit))
        .route("/api/explorer", get(api::explorer))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}

pub async fn run_sync_loop(state: NodeState, interval: Duration) {
    loop {
        let _result = state.synchronize().await;
        tokio::time::sleep(interval).await;
    }
}
