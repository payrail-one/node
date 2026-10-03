use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header::AUTHORIZATION},
};
use checkout_approval_core::ApprovalCodeState;

use crate::{
    AppState, DevnetError,
    model::{
        AccountStateView, ApprovalCodeChallengeView, ApprovalCodeClaimView, CheckoutView,
        ClaimApprovalCodeRequest, CreateCheckoutRequest, ExplorerOverviewView, FaucetRequest,
        IssueApprovalCodeRequest, IssuedApprovalCodeView, LiveEvent, NetworkStatusView,
        SubmissionResultView, SubmitRequest,
    },
    quorum::{QuorumProposal, QuorumValidatorStatus, QuorumVote},
};
use crate::{
    checkout::{now_ms, parse_id},
    codec::decode_bounded_hex,
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

pub async fn issue_approval_code(
    State(state): State<AppState>,
    Json(request): Json<IssueApprovalCodeRequest>,
) -> Result<Json<IssuedApprovalCodeView>, DevnetError> {
    let account_address = request.account_address;
    let account = with_service(state.clone(), move |service| {
        service.decode_account(&account_address)
    })
    .await?;
    let device = fixed_hex::<32>(&request.device_id)?;
    let nonce = fixed_hex::<32>(&request.nonce)?;
    let signature = fixed_hex::<64>(&request.signature)?;
    let issued_at_ms = request
        .issued_at_ms
        .parse::<u64>()
        .map_err(|_| DevnetError::ApprovalInvalid)?;
    tokio::task::spawn_blocking(move || {
        let rail = state
            .approval
            .as_ref()
            .ok_or(DevnetError::ApprovalUnavailable)?
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        rail.issue(account, device, issued_at_ms, nonce, signature, now_ms()?)
            .map(Json)
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}

pub async fn claim_approval_code(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ClaimApprovalCodeRequest>,
) -> Result<Json<ApprovalCodeClaimView>, DevnetError> {
    let checkout_id = parse_id(&id)?;
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let service = state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        if service.checkout(&id)?.status != "open" {
            return Err(DevnetError::CheckoutConflict);
        }
        let rail = state
            .approval
            .as_ref()
            .ok_or(DevnetError::ApprovalUnavailable)?
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        rail.authorize_merchant(authorization.as_deref())?;
        rail.claim(&request.code, checkout_id, now_ms()?)?;
        Ok(Json(ApprovalCodeClaimView {
            status: "claimed",
            checkout_id: id,
        }))
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}

pub async fn approval_code_challenge(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<ApprovalCodeChallengeView>, DevnetError> {
    tokio::task::spawn_blocking(move || {
        let record = state
            .approval
            .as_ref()
            .ok_or(DevnetError::ApprovalUnavailable)?
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?
            .record_for_session(&token, now_ms()?)?;
        let (mut status, checkout_id) = match record.state() {
            ApprovalCodeState::Ready => ("waiting", None),
            ApprovalCodeState::Claimed(claim) => ("claimed", Some(claim.checkout)),
            ApprovalCodeState::Consumed { claim, .. } => ("finalized", Some(claim.checkout)),
        };
        let checkout = checkout_id
            .map(|checkout| {
                state
                    .service
                    .lock()
                    .map_err(|_| DevnetError::StateUnavailable)?
                    .checkout(&crate::codec::encode_hex(checkout.as_bytes()))
            })
            .transpose()?;
        if status == "claimed"
            && checkout
                .as_ref()
                .is_some_and(|current| current.status == "finalized")
        {
            state
                .approval
                .as_ref()
                .ok_or(DevnetError::ApprovalUnavailable)?
                .lock()
                .map_err(|_| DevnetError::StateUnavailable)?
                .consume(
                    checkout_id.ok_or(DevnetError::InternalInvariant)?,
                    record.account(),
                    now_ms()?,
                )?;
            status = "finalized";
        }
        Ok(Json(ApprovalCodeChallengeView { status, checkout }))
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)?
}

pub async fn submit_checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<SubmitRequest>,
) -> Result<Json<CheckoutView>, DevnetError> {
    let checkout_id = parse_id(&id)?;
    let operation_state = state.clone();
    let checkout = tokio::task::spawn_blocking(move || {
        let mut service = operation_state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        let mut approval = operation_state
            .approval
            .as_ref()
            .map(|rail| rail.lock().map_err(|_| DevnetError::StateUnavailable))
            .transpose()?;
        let expected_payer = approval
            .as_ref()
            .map(|rail| rail.record_for_checkout(checkout_id))
            .transpose()?
            .flatten()
            .map(checkout_approval_core::ApprovalCodeRecord::account);
        let checkout = service.submit_checkout_bound(&id, &request.envelope, expected_payer)?;
        if let (Some(rail), Some(account)) = (approval.as_mut(), expected_payer) {
            rail.consume(checkout_id, account, now_ms()?)?;
        }
        Ok(checkout)
    })
    .await
    .map_err(|_| DevnetError::StateUnavailable)??;
    state.publish(LiveEvent::CheckoutUpdated {
        checkout: checkout.clone(),
    });
    Ok(Json(checkout))
}

fn fixed_hex<const LENGTH: usize>(value: &str) -> Result<[u8; LENGTH], DevnetError> {
    decode_bounded_hex(value, LENGTH)?
        .try_into()
        .map_err(|_| DevnetError::ApprovalInvalid)
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
