use std::time::Duration;

use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmBlockObservationProvider, EvmChainId,
    EvmFinalityProvider, EvmLog, EvmObservedBlock, EvmProviderFinality, EvmProviderId,
    MAX_DEPOSITS_PER_BLOCK,
};
use reqwest::{StatusCode, Url, blocking::Client, redirect::Policy};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    EvmRpcError,
    hex::{decode_quantity, encode_address, encode_hash, encode_quantity},
    response::{RpcBlock, bounded_body, decode_log, decode_result},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_FINALITY_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_LOG_RESPONSE_BYTES: usize = 4 * 1_024 * 1_024;

#[derive(Clone)]
pub struct RpcEvmFinalityProvider {
    id: EvmProviderId,
    expected_chain: EvmChainId,
    endpoint: Url,
    client: Client,
}

impl std::fmt::Debug for RpcEvmFinalityProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RpcEvmFinalityProvider")
            .field("id", &self.id)
            .field("expected_chain", &self.expected_chain)
            .field("endpoint", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl RpcEvmFinalityProvider {
    /// Creates a redirect-free HTTPS JSON-RPC provider. Endpoint credentials,
    /// if any, remain encapsulated and are never included in returned errors.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-HTTPS endpoint, URL credentials/fragments or
    /// HTTP/TLS client initialization failure.
    pub fn new(
        id: EvmProviderId,
        expected_chain: EvmChainId,
        endpoint: &str,
    ) -> Result<Self, EvmRpcError> {
        if id.is_zero() {
            return Err(EvmRpcError::InvalidEndpoint);
        }
        let endpoint = Url::parse(endpoint).map_err(|_| EvmRpcError::InvalidEndpoint)?;
        if endpoint.scheme() != "https"
            || !endpoint.has_host()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(EvmRpcError::InvalidEndpoint);
        }
        // The workspace standardizes Rustls on ring. Installation is process
        // global and idempotent when another client initialized it first.
        let _provider = rustls::crypto::ring::default_provider().install_default();
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(Policy::none())
            .build()
            .map_err(|_| EvmRpcError::ClientInitialization)?;
        Ok(Self {
            id,
            expected_chain,
            endpoint,
            client,
        })
    }

    fn chain_id(&self) -> Result<EvmChainId, EvmRpcError> {
        let result = self.call(1, "eth_chainId", json!([]), MAX_FINALITY_RESPONSE_BYTES)?;
        let encoded = result.as_str().ok_or(EvmRpcError::InvalidResponse)?;
        EvmChainId::new(decode_quantity(encoded)?).ok_or(EvmRpcError::InvalidResponse)
    }

    fn block(&self, id: u64, tag: &str) -> Result<Option<RpcBlock>, EvmRpcError> {
        let result = self.call(
            id,
            "eth_getBlockByNumber",
            json!([tag, false]),
            MAX_FINALITY_RESPONSE_BYTES,
        )?;
        if result.is_null() {
            return Ok(None);
        }
        RpcBlock::decode(&result).map(Some)
    }

    fn deposit_logs(
        &self,
        id: u64,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Vec<EvmLog>, EvmRpcError> {
        let block = encode_quantity(block_number);
        let result = self.call(
            id,
            "eth_getLogs",
            json!([{
                "address": encode_address(bridge_contract),
                "fromBlock": block,
                "toBlock": block,
                "topics": [encode_hash(DEPOSITED_EVENT_TOPIC)]
            }]),
            MAX_LOG_RESPONSE_BYTES,
        )?;
        let logs = result.as_array().ok_or(EvmRpcError::InvalidResponse)?;
        if logs.len() > MAX_DEPOSITS_PER_BLOCK {
            return Err(EvmRpcError::InvalidResponse);
        }
        logs.iter().map(decode_log).collect()
    }

    fn call(
        &self,
        id: u64,
        method: &str,
        params: Value,
        maximum_response_bytes: usize,
    ) -> Result<Value, EvmRpcError> {
        let request = RpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let response = self
            .client
            .post(self.endpoint.clone())
            .json(&request)
            .send()
            .map_err(|_| EvmRpcError::Transport)?;
        if response.status() != StatusCode::OK {
            return Err(EvmRpcError::Rejected);
        }
        let body = bounded_body(response, maximum_response_bytes)?;
        decode_result(&body, id)
    }
}

impl EvmFinalityProvider for RpcEvmFinalityProvider {
    type Error = EvmRpcError;

    fn provider_id(&self) -> EvmProviderId {
        self.id
    }

    fn finalized_view(
        &self,
        chain_id: EvmChainId,
        block_number: u64,
    ) -> Result<Option<EvmProviderFinality>, Self::Error> {
        if chain_id != self.expected_chain || self.chain_id()? != self.expected_chain {
            return Err(EvmRpcError::WrongChain);
        }
        let finalized = self
            .block(2, "finalized")?
            .ok_or(EvmRpcError::MissingBlock)?;
        if finalized.number < block_number {
            return Ok(None);
        }
        let requested = self
            .block(3, &encode_quantity(block_number))?
            .ok_or(EvmRpcError::MissingBlock)?;
        if requested.number != block_number {
            return Err(EvmRpcError::InvalidResponse);
        }
        Ok(Some(EvmProviderFinality {
            block_number,
            block_hash: requested.hash,
            finalized_head_number: finalized.number,
            finalized_head_hash: finalized.hash,
        }))
    }
}

impl EvmBlockObservationProvider for RpcEvmFinalityProvider {
    type Error = EvmRpcError;

    fn provider_id(&self) -> EvmProviderId {
        self.id
    }

    fn observed_block(
        &self,
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error> {
        if bridge_contract.is_zero() {
            return Err(EvmRpcError::InvalidBridgeContract);
        }
        if chain_id != self.expected_chain || self.chain_id()? != self.expected_chain {
            return Err(EvmRpcError::WrongChain);
        }
        let Some(block) = self.block(10, &encode_quantity(block_number))? else {
            return Ok(None);
        };
        if block.number != block_number {
            return Err(EvmRpcError::InvalidResponse);
        }
        let logs = self.deposit_logs(11, bridge_contract, block_number)?;
        Ok(Some(EvmObservedBlock {
            number: block.number,
            hash: block.hash,
            parent_hash: block.parent_hash,
            logs,
        }))
    }
}

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: Value,
}

#[cfg(test)]
mod tests {
    use ethereum_bridge_core::{EvmChainId, EvmProviderId};

    use super::RpcEvmFinalityProvider;
    use crate::EvmRpcError;

    #[test]
    fn endpoints_require_https_without_embedded_authority_or_fragment() {
        let id = EvmProviderId::new([1; 32]);
        let chain = EvmChainId::new(1).unwrap();
        assert!(RpcEvmFinalityProvider::new(id, chain, "https://rpc.example/v1?k=x").is_ok());
        for invalid in [
            "http://rpc.example",
            "https://user:password@rpc.example",
            "https://rpc.example/#secret",
            "not a url",
        ] {
            assert_eq!(
                RpcEvmFinalityProvider::new(id, chain, invalid).unwrap_err(),
                EvmRpcError::InvalidEndpoint
            );
        }
        assert_eq!(
            RpcEvmFinalityProvider::new(EvmProviderId::new([0; 32]), chain, "https://rpc.example")
                .unwrap_err(),
            EvmRpcError::InvalidEndpoint
        );

        let provider = RpcEvmFinalityProvider::new(
            id,
            chain,
            "https://rpc.example/private?api-key=do-not-log",
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("do-not-log"));
    }
}
