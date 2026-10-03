use std::time::Duration;

use devnet_gateway::{SubmitRequest, SyncBlockView, SyncBootstrapView};
use reqwest::{Client, StatusCode, Url, redirect::Policy};

use crate::error::NodeError;

const MAX_SYNC_RESPONSE_BYTES: usize = 9 * 1024 * 1024;
const MAX_SUBMIT_RESPONSE_BYTES: usize = 128 * 1024;

#[derive(Clone)]
pub struct UpstreamClient {
    client: Client,
    origins: Vec<Url>,
}

impl UpstreamClient {
    /// Builds a bounded, redirect-free HTTP client for the supplied origins.
    ///
    /// # Errors
    ///
    /// Returns an error when the TLS or HTTP client cannot be initialized.
    pub fn new(origins: Vec<Url>) -> Result<Self, NodeError> {
        let _provider = rustls::crypto::ring::default_provider().install_default();
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .redirect(Policy::none())
            .build()
            .map_err(|_| NodeError::Configuration("HTTP client initialization failed"))?;
        Ok(Self { client, origins })
    }

    #[must_use]
    pub fn origins(&self) -> &[Url] {
        &self.origins
    }

    /// Fetches the bounded synchronization identity and tip.
    ///
    /// # Errors
    ///
    /// Returns an error for transport, status, size or JSON failures.
    pub async fn bootstrap(&self, origin: &Url) -> Result<SyncBootstrapView, NodeError> {
        self.get_json(origin, "api/sync/bootstrap").await
    }

    /// Fetches one bounded finalized block by height.
    ///
    /// # Errors
    ///
    /// Returns an error for transport, status, size or JSON failures.
    pub async fn block(&self, origin: &Url, height: u64) -> Result<SyncBlockView, NodeError> {
        self.get_json(origin, &format!("api/sync/blocks/{height}"))
            .await
    }

    /// Relays a client-signed transaction without retrying an ambiguous POST.
    ///
    /// # Errors
    ///
    /// Returns an error for transport, malformed responses or upstream rejection.
    pub async fn submit(
        &self,
        request: &SubmitRequest,
    ) -> Result<(StatusCode, serde_json::Value), NodeError> {
        let origin = self.origins.first().ok_or(NodeError::UpstreamUnavailable)?;
        let url = origin
            .join("api/transactions")
            .map_err(|_| NodeError::InvalidUpstream)?;
        let response = self
            .client
            .post(url)
            .json(request)
            .send()
            .await
            .map_err(|_| NodeError::UpstreamUnavailable)?;
        let status = response.status();
        let body = bounded_body(response, MAX_SUBMIT_RESPONSE_BYTES).await?;
        let json = serde_json::from_slice(&body).map_err(|_| NodeError::InvalidUpstream)?;
        if status.is_success() {
            Ok((status, json))
        } else {
            Err(NodeError::UpstreamRejected(status, json))
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        origin: &Url,
        path: &str,
    ) -> Result<T, NodeError> {
        let url = origin.join(path).map_err(|_| NodeError::InvalidUpstream)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| NodeError::UpstreamUnavailable)?;
        if !response.status().is_success() {
            return Err(NodeError::UpstreamUnavailable);
        }
        let body = bounded_body(response, MAX_SYNC_RESPONSE_BYTES).await?;
        serde_json::from_slice(&body).map_err(|_| NodeError::InvalidUpstream)
    }
}

async fn bounded_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, NodeError> {
    if response.content_length().is_some_and(|length| {
        usize::try_from(length).map_or(true, |body_length| body_length > limit)
    }) {
        return Err(NodeError::InvalidUpstream);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| NodeError::UpstreamUnavailable)?
    {
        let length = body
            .len()
            .checked_add(chunk.len())
            .ok_or(NodeError::InvalidUpstream)?;
        if length > limit {
            return Err(NodeError::InvalidUpstream);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
