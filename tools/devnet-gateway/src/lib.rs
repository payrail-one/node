#![forbid(unsafe_code)]

mod api;
mod approval;
mod checkout;
mod codec;
mod contract_view;
mod error;
mod index;
mod live;
mod model;
mod persistence;
mod quorum;
mod replica;
mod service;
mod sync;

#[cfg(test)]
mod approval_tests;

use std::sync::{Arc, Mutex};

use axum::{
    Router,
    routing::{get, post},
};
use tokio::sync::broadcast;

use crate::{
    approval::ApprovalRail,
    model::LiveEvent,
    quorum::{QuorumCoordinator, QuorumSetup, QuorumSigner},
};

pub use error::DevnetError;
pub use model::{
    AccountStateView, ContractExecutionView, ContractStateEntryView, ContractView,
    ExplorerOverviewView, NetworkAssetView, NetworkStatusView, SubmissionResultView, SubmitRequest,
    SyncBlockView, SyncBootstrapView, SyncCheckpointView,
};
pub use service::DevnetService;

/// Returns the public validator key for one raw 32-byte seed file.
///
/// # Errors
///
/// Rejects missing, symlinked or incorrectly sized key material.
pub fn validator_public_key(path: impl AsRef<std::path::Path>) -> Result<String, DevnetError> {
    quorum::public_key(path.as_ref())
}

/// Returns the Payrail development address for one raw 32-byte account seed file.
///
/// # Errors
///
/// Rejects missing, symlinked or incorrectly sized key material.
pub fn account_address(path: impl AsRef<std::path::Path>) -> Result<String, DevnetError> {
    quorum::account_address(path.as_ref())
}

/// Continuously catches one validator up from the public finalized-block feed.
///
/// # Errors
///
/// Rejects an invalid upstream origin before starting the retry loop.
pub async fn run_replica_sync(state: AppState, upstream: String) -> Result<(), DevnetError> {
    replica::run(state, upstream).await
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) service: Arc<Mutex<DevnetService>>,
    approval: Option<Arc<Mutex<ApprovalRail>>>,
    events: broadcast::Sender<LiveEvent>,
    quorum_signer: Arc<Mutex<Option<QuorumSigner>>>,
    coordinator_public_key: Option<ed25519_dalek::VerifyingKey>,
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
        let path = path.as_ref();
        Ok(Self {
            service: Arc::new(Mutex::new(DevnetService::open(path)?)),
            approval: ApprovalRail::from_environment(path)?.map(|rail| Arc::new(Mutex::new(rail))),
            events,
            quorum_signer: Arc::new(Mutex::new(None)),
            coordinator_public_key: None,
        })
    }

    /// Opens either the legacy local mode or a configured quorum validator.
    ///
    /// # Errors
    ///
    /// Fails closed when validator membership, key material or endpoints are invalid.
    pub fn open_configured(path: impl AsRef<std::path::Path>) -> Result<Self, DevnetError> {
        let path = path.as_ref();
        let Some(setup) = QuorumSetup::from_environment(path)? else {
            return Self::open(path);
        };
        let (events, _) = broadcast::channel(256);
        let QuorumSetup {
            policy,
            signer,
            coordinator_public_key,
            endpoints,
        } = setup;
        if endpoints.is_empty() {
            return Ok(Self {
                service: Arc::new(Mutex::new(DevnetService::open_with(path, policy, None)?)),
                approval: ApprovalRail::from_environment(path)?
                    .map(|rail| Arc::new(Mutex::new(rail))),
                events,
                quorum_signer: Arc::new(Mutex::new(Some(signer))),
                coordinator_public_key: Some(coordinator_public_key),
            });
        }
        let coordinator = QuorumCoordinator::new(signer, endpoints)?;
        Ok(Self {
            service: Arc::new(Mutex::new(DevnetService::open_with(
                path,
                policy,
                Some(coordinator),
            )?)),
            approval: ApprovalRail::from_environment(path)?.map(|rail| Arc::new(Mutex::new(rail))),
            events,
            quorum_signer: Arc::new(Mutex::new(None)),
            coordinator_public_key: Some(coordinator_public_key),
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
        .route("/api/contracts/{id}", get(api::contract))
        .route("/api/explorer", get(api::explorer))
        .route("/api/live", get(live::upgrade))
        .route("/api/sync/bootstrap", get(api::sync_bootstrap))
        .route("/api/sync/blocks/{height}", get(api::sync_block))
        .route("/internal/quorum/status", get(api::quorum_status))
        .route("/internal/quorum/prepare", post(api::quorum_prepare))
        .route("/internal/quorum/commit", post(api::quorum_commit))
        .route("/api/checkouts", post(api::create_checkout))
        .route("/api/approval-codes", post(api::issue_approval_code))
        .route(
            "/api/approval-codes/sessions/{token}",
            get(api::approval_code_challenge),
        )
        .route("/api/checkouts/{id}", get(api::checkout))
        .route(
            "/api/checkouts/{id}/approval-code",
            post(api::claim_approval_code),
        )
        .route(
            "/api/checkouts/{id}/transactions",
            post(api::submit_checkout),
        )
        .with_state(state)
}
