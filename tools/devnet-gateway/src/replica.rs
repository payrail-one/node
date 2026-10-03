use std::time::Duration;

use reqwest::{Client, Url, redirect::Policy};
use serde::de::DeserializeOwned;

use crate::{AppState, DevnetError, SyncBlockView, SyncBootstrapView};

const MAX_BOOTSTRAP_BYTES: usize = 16 * 1024;
const MAX_BLOCK_BYTES: usize = 9 * 1024 * 1024;

pub(crate) async fn run(state: AppState, upstream: String) -> Result<(), DevnetError> {
    let origin = parse_origin(&upstream)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .redirect(Policy::none())
        .build()
        .map_err(|_| DevnetError::InvalidQuorumConfiguration)?;
    loop {
        let _result = synchronize(&state, &client, &origin).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn synchronize(state: &AppState, client: &Client, origin: &Url) -> Result<(), DevnetError> {
    let remote: SyncBootstrapView = get_json(
        client,
        origin
            .join("api/sync/bootstrap")
            .map_err(|_| DevnetError::InvalidQuorumConfiguration)?,
        MAX_BOOTSTRAP_BYTES,
    )
    .await?;
    let (local, mut height) = {
        let service = state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?;
        (service.sync_bootstrap()?, service.finalized_height())
    };
    if remote.network_id != local.network_id
        || remote.genesis != local.genesis
        || remote.finality_mode != local.finality_mode
    {
        return Err(DevnetError::InvalidSyncBlock);
    }
    let target = remote
        .finalized_height
        .parse::<u64>()
        .map_err(|_| DevnetError::InvalidSyncBlock)?;
    while height < target {
        let next = height
            .checked_add(1)
            .ok_or(DevnetError::InternalInvariant)?;
        let block: SyncBlockView = get_json(
            client,
            origin
                .join(&format!("api/sync/blocks/{next}"))
                .map_err(|_| DevnetError::InvalidQuorumConfiguration)?,
            MAX_BLOCK_BYTES,
        )
        .await?;
        state
            .service
            .lock()
            .map_err(|_| DevnetError::StateUnavailable)?
            .apply_sync_block(&block)?;
        height = next;
    }
    Ok(())
}

async fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: Url,
    limit: usize,
) -> Result<T, DevnetError> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| DevnetError::QuorumUnavailable)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| usize::try_from(length).map_or(true, |length| length > limit))
    {
        return Err(DevnetError::QuorumUnavailable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| DevnetError::QuorumUnavailable)?
    {
        let length = body
            .len()
            .checked_add(chunk.len())
            .ok_or(DevnetError::InvalidSyncBlock)?;
        if length > limit {
            return Err(DevnetError::InvalidSyncBlock);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| DevnetError::InvalidSyncBlock)
}

fn parse_origin(value: &str) -> Result<Url, DevnetError> {
    let mut url = Url::parse(value).map_err(|_| DevnetError::InvalidQuorumConfiguration)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(DevnetError::InvalidQuorumConfiguration);
    }
    url.set_path("/");
    Ok(url)
}
