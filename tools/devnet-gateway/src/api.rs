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
};

pub async fn status(State(state): State<AppState>) -> Result<Json<NetworkStatusView>, DevnetError> {
    with_service(&state, |service| Ok(service.status())).map(Json)
}

pub async fn account(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<AccountStateView>, DevnetError> {
    with_service(&state, |service| service.account(&address)).map(Json)
}

pub async fn faucet(
    State(state): State<AppState>,
    Json(request): Json<FaucetRequest>,
) -> Result<Json<SubmissionResultView>, DevnetError> {
    let result = with_service(&state, |service| service.faucet(&request.address))?;
    publish_finalized(&state, &result);
    Ok(Json(result))
}

pub async fn submit(
    State(state): State<AppState>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<SubmissionResultView>, DevnetError> {
    let result = with_service(&state, |service| service.submit_hex(&request.envelope))?;
    publish_finalized(&state, &result);
    Ok(Json(result))
}

pub async fn explorer(
    State(state): State<AppState>,
) -> Result<Json<ExplorerOverviewView>, DevnetError> {
    with_service(&state, |service| Ok(service.explorer())).map(Json)
}

pub async fn sync_bootstrap(
    State(state): State<AppState>,
) -> Result<Json<crate::SyncBootstrapView>, DevnetError> {
    with_service(&state, |service| service.sync_bootstrap()).map(Json)
}

pub async fn sync_block(
    State(state): State<AppState>,
    Path(height): Path<u64>,
) -> Result<Json<crate::SyncBlockView>, DevnetError> {
    with_service(&state, |service| service.sync_block(height)).map(Json)
}

pub async fn create_checkout(
    State(state): State<AppState>,
    Json(request): Json<CreateCheckoutRequest>,
) -> Result<Json<CheckoutView>, DevnetError> {
    let checkout = with_service(&state, |service| {
        service.create_checkout(
            &request.merchant_address,
            &request.amount,
            &request.order_reference,
        )
    })?;
    state.publish(LiveEvent::CheckoutUpdated {
        checkout: checkout.clone(),
    });
    Ok(Json(checkout))
}

pub async fn checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CheckoutView>, DevnetError> {
    with_service(&state, |service| service.checkout(&id)).map(Json)
}

pub async fn submit_checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<CheckoutView>, DevnetError> {
    let checkout = with_service(&state, |service| {
        service.submit_checkout(&id, &request.envelope)
    })?;
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

fn with_service<T>(
    state: &AppState,
    operation: impl FnOnce(&mut crate::DevnetService) -> Result<T, DevnetError>,
) -> Result<T, DevnetError> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| DevnetError::StateUnavailable)?;
    operation(&mut service)
}
