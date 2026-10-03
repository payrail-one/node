use std::sync::{Arc, Mutex};

use devnet_gateway::{
    AccountStateView, DevnetError, DevnetService, ExplorerOverviewView, NetworkStatusView,
    SubmitRequest,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::{error::NodeError, upstream::UpstreamClient};

#[derive(Clone, Debug, Default)]
pub struct SyncHealth {
    pub target_height: Option<u64>,
    pub active_upstream: Option<String>,
    pub last_error: Option<String>,
    pub synchronized: bool,
}

#[derive(Clone)]
pub struct NodeState {
    service: Arc<Mutex<DevnetService>>,
    upstream: UpstreamClient,
    health: Arc<Mutex<SyncHealth>>,
    sync_gate: Arc<AsyncMutex<()>>,
}

impl NodeState {
    /// Opens the durable local replica state.
    ///
    /// # Errors
    ///
    /// Returns an error when the local ledger cannot be opened or recovered.
    pub fn open(
        path: impl AsRef<std::path::Path>,
        upstream: UpstreamClient,
    ) -> Result<Self, NodeError> {
        Ok(Self {
            service: Arc::new(Mutex::new(
                DevnetService::open(path).map_err(|_| NodeError::LocalState)?,
            )),
            upstream,
            health: Arc::new(Mutex::new(SyncHealth::default())),
            sync_gate: Arc::new(AsyncMutex::new(())),
        })
    }

    /// Returns the local network and synchronization status.
    ///
    /// # Errors
    ///
    /// Returns an error if local state locks are unavailable.
    pub fn status(&self) -> Result<(NetworkStatusView, SyncHealth), NodeError> {
        let service = self.service.lock().map_err(|_| NodeError::LocalState)?;
        let health = self.health.lock().map_err(|_| NodeError::LocalState)?;
        Ok((service.status(), health.clone()))
    }

    /// Reads one account from locally verified finalized state.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid address or unavailable local state.
    pub fn account(&self, address: &str) -> Result<AccountStateView, NodeError> {
        self.service
            .lock()
            .map_err(|_| NodeError::LocalState)?
            .account(address)
            .map_err(|error| match error {
                DevnetError::InvalidAddress => {
                    NodeError::InvalidRequest("invalid Payrail development-network address")
                }
                _ => NodeError::LocalState,
            })
    }

    /// Reads the local finalized explorer projection.
    ///
    /// # Errors
    ///
    /// Returns an error if local state is unavailable.
    pub fn explorer(&self) -> Result<ExplorerOverviewView, NodeError> {
        Ok(self
            .service
            .lock()
            .map_err(|_| NodeError::LocalState)?
            .explorer())
    }

    /// Relays one client-signed envelope to the configured primary upstream.
    ///
    /// # Errors
    ///
    /// Returns an error for transport failures or upstream rejection.
    pub async fn submit(
        &self,
        request: &SubmitRequest,
    ) -> Result<(reqwest::StatusCode, serde_json::Value), NodeError> {
        self.upstream.submit(request).await
    }

    /// Catches up sequentially from the first valid available upstream.
    ///
    /// # Errors
    ///
    /// Returns an error if every upstream is unavailable or supplies invalid data.
    pub async fn synchronize(&self) -> Result<u64, NodeError> {
        let _guard = self.sync_gate.lock().await;
        let result = self.synchronize_from_any().await;
        let mut health = self.health.lock().map_err(|_| NodeError::LocalState)?;
        match &result {
            Ok(height) => {
                health.target_height = Some(*height);
                health.last_error = None;
                health.synchronized = true;
            }
            Err(error) => {
                health.last_error = Some(error.to_string());
                health.synchronized = false;
            }
        }
        result
    }

    async fn synchronize_from_any(&self) -> Result<u64, NodeError> {
        let mut invalid_response = false;
        for origin in self.upstream.origins() {
            match self.synchronize_from(origin).await {
                Ok(height) => {
                    let mut health = self.health.lock().map_err(|_| NodeError::LocalState)?;
                    health.active_upstream = Some(origin.origin().ascii_serialization());
                    return Ok(height);
                }
                Err(NodeError::InvalidUpstream) => invalid_response = true,
                Err(NodeError::LocalState) => return Err(NodeError::LocalState),
                Err(_) => {}
            }
        }
        if invalid_response {
            Err(NodeError::InvalidUpstream)
        } else {
            Err(NodeError::UpstreamUnavailable)
        }
    }

    async fn synchronize_from(&self, origin: &reqwest::Url) -> Result<u64, NodeError> {
        let remote = self.upstream.bootstrap(origin).await?;
        let (local, mut height) = {
            let service = self.service.lock().map_err(|_| NodeError::LocalState)?;
            (
                service
                    .sync_bootstrap()
                    .map_err(|_| NodeError::LocalState)?,
                service.finalized_height(),
            )
        };
        if remote.network_id != local.network_id
            || remote.genesis != local.genesis
            || remote.finality_mode != local.finality_mode
        {
            return Err(NodeError::InvalidUpstream);
        }
        let target = remote
            .finalized_height
            .parse::<u64>()
            .map_err(|_| NodeError::InvalidUpstream)?;
        if target < height {
            return Err(NodeError::InvalidUpstream);
        }
        while height < target {
            let next = height.checked_add(1).ok_or(NodeError::InvalidUpstream)?;
            let block = self.upstream.block(origin, next).await?;
            self.service
                .lock()
                .map_err(|_| NodeError::LocalState)?
                .apply_sync_block(&block)
                .map_err(|error| match error {
                    DevnetError::StateUnavailable | DevnetError::InternalInvariant => {
                        NodeError::LocalState
                    }
                    _ => NodeError::InvalidUpstream,
                })?;
            height = next;
        }
        Ok(height)
    }
}
