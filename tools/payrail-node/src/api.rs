use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use devnet_gateway::{AccountStateView, ExplorerOverviewView, SubmitRequest};
use serde::Serialize;

use crate::{error::NodeError, state::NodeState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNodeStatusView {
    role: &'static str,
    network_id: String,
    finality_mode: String,
    local_finalized_height: String,
    upstream_finalized_height: Option<String>,
    synchronized: bool,
    active_upstream: Option<String>,
    last_error: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct HealthView {
    status: &'static str,
}

pub async fn status(
    State(state): State<NodeState>,
) -> Result<Json<PublicNodeStatusView>, NodeError> {
    let (network, health) = state.status()?;
    Ok(Json(PublicNodeStatusView {
        role: "public-replica",
        network_id: network.network_id,
        finality_mode: network.finality_mode.to_owned(),
        local_finalized_height: network.finalized_height,
        upstream_finalized_height: health.target_height.map(|height| height.to_string()),
        synchronized: health.synchronized,
        active_upstream: health.active_upstream,
        last_error: health.last_error,
    }))
}

pub async fn account(
    State(state): State<NodeState>,
    Path(address): Path<String>,
) -> Result<Json<AccountStateView>, NodeError> {
    state.account(&address).map(Json)
}

pub async fn explorer(
    State(state): State<NodeState>,
) -> Result<Json<ExplorerOverviewView>, NodeError> {
    state.explorer().map(Json)
}

pub async fn submit(
    State(state): State<NodeState>,
    Json(request): Json<SubmitRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), NodeError> {
    let (status, response) = state.submit(&request).await?;
    let sync_state = state.clone();
    let _task = tokio::spawn(async move {
        let _result = sync_state.synchronize().await;
    });
    Ok((status, Json(response)))
}

pub async fn live() -> Json<HealthView> {
    Json(HealthView { status: "live" })
}

pub async fn ready(State(state): State<NodeState>) -> Result<Json<HealthView>, NodeError> {
    let (_, health) = state.status()?;
    if !health.synchronized {
        return Err(NodeError::NotSynchronized);
    }
    Ok(Json(HealthView { status: "ready" }))
}
