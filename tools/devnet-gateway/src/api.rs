use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    AppState, DevnetError,
    model::{
        AccountStateView, CheckoutView, CreateCheckoutRequest, ExplorerOverviewView, FaucetRequest,
        LiveEvent, NetworkStatusView, SubmissionResultView, SubmitRequest,
    },
    quorum::{QuorumProposal, QuorumValidatorStatus, QuorumVote},
};

pub async fn status(State(state): State<AppState>) -> Result<Json<NetworkStatusView>, DevnetError> {
    with_service(state, |service| Ok(service.status()))
        .await
        .map(Json)
}

pub async fn account(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<AccountStateView>, DevnetError> {
    with_service(state, move |service| service.account(&address))
        .await
        .map(Json)
}

pub async fn faucet(
    State(state): State<AppState>,
    Json(request): Json<FaucetRequest>,
) -> Result<Json<SubmissionResultView>, DevnetError> {
    let operation_state = state.clone();
    let result = with_service(operation_state, move |service| {
        service.faucet(&request.address)
    })
    .await?;
    publish_finalized(&state, &result);
    Ok(Json(result))
}

pub async fn submit(
    State(state): State<AppState>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<SubmissionResultView>, DevnetError> {
    let operation_state = state.clone();
    let result = with_service(operation_state, move |service| {
        service.submit_hex(&request.envelope)
    })
    .await?;
    publish_finalized(&state, &result);
    Ok(Json(result))
}

pub async fn explorer(
    State(state): State<AppState>,
) -> Result<Json<ExplorerOverviewView>, DevnetError> {
    with_service(state, |service| Ok(service.explorer()))
        .await
        .map(Json)
}

pub async fn sync_bootstrap(
    State(state): State<AppState>,
) -> Result<Json<crate::SyncBootstrapView>, DevnetError> {
    with_service(state, |service| service.sync_bootstrap())
        .await
        .map(Json)
}

pub async fn sync_block(
    State(state): State<AppState>,
    Path(height): Path<u64>,
) -> Result<Json<crate::SyncBlockView>, DevnetError> {
    with_service(state, move |service| service.sync_block(height))
        .await
        .map(Json)
}

pub async fn quorum_status(
    State(state): State<AppState>,
) -> Result<Json<QuorumValidatorStatus>, DevnetError> {
    tokio::task::spawn_blocking(move || {
        let service = state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        let signer = state
            .quorum_signer
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        let signer = signer.as_ref().ok_or(DevnetError::InvalidQuorumRequest)?;
        Ok(Json(QuorumValidatorStatus {
            node: crate::codec::encode_hex(signer.node().as_bytes()),
            finalized_height: service.finalized_height().to_string(),
        }))
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}

pub async fn quorum_prepare(
    State(state): State<AppState>,
    Json(proposal): Json<QuorumProposal>,
) -> Result<Json<QuorumVote>, DevnetError> {
    tokio::task::spawn_blocking(move || {
        let coordinator = state
            .coordinator_public_key
            .as_ref()
            .ok_or(DevnetError::InvalidQuorumRequest)?;
        proposal.verify_authorization(coordinator)?;
        let checkpoint = {
            let service = state
                .service
                .lock()
                .map_err(|_| DevnetError::StateUnavailable)?;
            service.validate_quorum_proposal(&proposal)?
        };
        let signature = state
            .quorum_signer
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?
            .as_mut()
            .ok_or(DevnetError::InvalidQuorumRequest)?
            .sign_checkpoint(checkpoint)?;
        Ok(Json(signature.into()))
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}

pub async fn quorum_commit(
    State(state): State<AppState>,
    Json(block): Json<crate::SyncBlockView>,
) -> Result<Json<SubmissionResultView>, DevnetError> {
    with_service(state, move |service| service.apply_sync_block(&block))
        .await
        .map(Json)
}

pub async fn create_checkout(
    State(state): State<AppState>,
    Json(request): Json<CreateCheckoutRequest>,
) -> Result<Json<CheckoutView>, DevnetError> {
    let operation_state = state.clone();
    let checkout = with_service(operation_state, move |service| {
        service.create_checkout(
            &request.merchant_address,
            &request.amount,
            &request.order_reference,
        )
    })
    .await?;
    state.publish(LiveEvent::CheckoutUpdated {
        checkout: checkout.clone(),
    });
    Ok(Json(checkout))
}

pub async fn checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CheckoutView>, DevnetError> {
    with_service(state, move |service| service.checkout(&id))
        .await
        .map(Json)
}

pub async fn submit_checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<CheckoutView>, DevnetError> {
    let operation_state = state.clone();
    let checkout = with_service(operation_state, move |service| {
        service.submit_checkout(&id, &request.envelope)
    })
    .await?;
    state.publish(LiveEvent::CheckoutUpdated {
        checkout: checkout.clone(),
    });
    Ok(Json(checkout))
}

fn publish_finalized(state: &AppState, result: &SubmissionResultView) {
    state.publish(LiveEvent::Finalized {
        transaction: result.transaction.clone(),
        checkpoint: result.checkpoint.clone(),
    });
}

async fn with_service<T: Send + 'static>(
    state: AppState,
    operation: impl FnOnce(&mut crate::DevnetService) -> Result<T, DevnetError> + Send + 'static,
) -> Result<T, DevnetError> {
    tokio::task::spawn_blocking(move || {
        let mut service = state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        operation(&mut service)
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}
