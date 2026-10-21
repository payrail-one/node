use std::io::Read;

use ethereum_bridge_core::{EvmHash, EvmLog};
use serde_json::Value;

use crate::{
    EvmRpcError,
    hex::{decode_address, decode_bytes, decode_hash, decode_quantity},
};

const MAX_LOG_DATA_BYTES: usize = 4 * 1_024;
const MAX_TOPICS_PER_LOG: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RpcBlock {
    pub(crate) number: u64,
    pub(crate) hash: EvmHash,
    pub(crate) parent_hash: EvmHash,
}

impl RpcBlock {
    pub(crate) fn decode(value: &Value) -> Result<Self, EvmRpcError> {
        let object = value.as_object().ok_or(EvmRpcError::InvalidResponse)?;
        Ok(Self {
            number: required_string(object, "number").and_then(decode_quantity)?,
            hash: required_string(object, "hash").and_then(decode_hash)?,
            parent_hash: required_string(object, "parentHash").and_then(decode_hash)?,
        })
    }
}

pub(crate) fn bounded_body(
    response: reqwest::blocking::Response,
    maximum_response_bytes: usize,
) -> Result<Vec<u8>, EvmRpcError> {
    if response.content_length().is_some_and(|length| {
        usize::try_from(length).map_or(true, |value| value > maximum_response_bytes)
    }) {
        return Err(EvmRpcError::ResponseTooLarge);
    }
    let maximum = u64::try_from(maximum_response_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut body = Vec::new();
    response
        .take(maximum)
        .read_to_end(&mut body)
        .map_err(|_| EvmRpcError::Transport)?;
    if body.len() > maximum_response_bytes {
        return Err(EvmRpcError::ResponseTooLarge);
    }
    Ok(body)
}

pub(crate) fn decode_log(value: &Value) -> Result<EvmLog, EvmRpcError> {
    let object = value.as_object().ok_or(EvmRpcError::InvalidResponse)?;
    let address = required_string(object, "address").and_then(decode_address)?;
    let raw_topics = object
        .get("topics")
        .and_then(Value::as_array)
        .ok_or(EvmRpcError::InvalidResponse)?;
    if raw_topics.len() > MAX_TOPICS_PER_LOG {
        return Err(EvmRpcError::InvalidResponse);
    }
    let topics = raw_topics
        .iter()
        .map(|topic| {
            topic
                .as_str()
                .ok_or(EvmRpcError::InvalidResponse)
                .and_then(decode_hash)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let data = required_string(object, "data")
        .and_then(|encoded| decode_bytes(encoded, MAX_LOG_DATA_BYTES))?;
    let transaction_hash = required_string(object, "transactionHash").and_then(decode_hash)?;
    let block_hash = required_string(object, "blockHash").and_then(decode_hash)?;
    let block_number = required_string(object, "blockNumber").and_then(decode_quantity)?;
    let log_index = required_string(object, "logIndex").and_then(decode_quantity)?;
    let removed = object
        .get("removed")
        .and_then(Value::as_bool)
        .ok_or(EvmRpcError::InvalidResponse)?;
    Ok(EvmLog {
        address,
        topics,
        data,
        transaction_hash: Some(transaction_hash),
        block_hash: Some(block_hash),
        block_number: Some(block_number),
        log_index: Some(log_index),
        removed,
    })
}

fn required_string<'a>(
    object: &'a serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a str, EvmRpcError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or(EvmRpcError::InvalidResponse)
}

pub(crate) fn decode_result(body: &[u8], expected_id: u64) -> Result<Value, EvmRpcError> {
    let response: Value = serde_json::from_slice(body).map_err(|_| EvmRpcError::InvalidResponse)?;
    let object = response.as_object().ok_or(EvmRpcError::InvalidResponse)?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || object.get("id").and_then(Value::as_u64) != Some(expected_id)
    {
        return Err(EvmRpcError::InvalidResponse);
    }
    if object.get("error").is_some_and(|error| !error.is_null()) {
        return Err(EvmRpcError::RemoteError);
    }
    object
        .get("result")
        .cloned()
        .ok_or(EvmRpcError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use ethereum_bridge_core::{DEPOSITED_EVENT_TOPIC, EvmAddress, EvmHash};
    use serde_json::json;

    use super::{decode_log, decode_result};
    use crate::{
        EvmRpcError,
        hex::{encode_address, encode_hash},
    };

    #[test]
    fn rpc_envelope_is_bound_to_version_and_request_id() {
        let valid = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": "0x1"
        }))
        .unwrap();
        assert_eq!(decode_result(&valid, 7), Ok(json!("0x1")));
        assert_eq!(decode_result(&valid, 8), Err(EvmRpcError::InvalidResponse));

        let wrong_version = serde_json::to_vec(&json!({
            "jsonrpc": "1.0",
            "id": 7,
            "result": "0x1"
        }))
        .unwrap();
        assert_eq!(
            decode_result(&wrong_version, 7),
            Err(EvmRpcError::InvalidResponse)
        );
    }

    #[test]
    fn remote_error_or_ambiguous_result_fails_closed() {
        let remote_error = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "error": {"code": -32000, "message": "unavailable"}
        }))
        .unwrap();
        assert_eq!(
            decode_result(&remote_error, 7),
            Err(EvmRpcError::RemoteError)
        );

        let both = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": "0x1",
            "error": {"code": -1}
        }))
        .unwrap();
        assert_eq!(decode_result(&both, 7), Err(EvmRpcError::RemoteError));
    }

    #[test]
    fn complete_log_metadata_decodes_strictly() {
        let address = EvmAddress::new([7; 20]);
        let transaction_hash = EvmHash::new([8; 32]);
        let block_hash = EvmHash::new([9; 32]);
        let value = json!({
            "address": encode_address(address),
            "topics": [encode_hash(DEPOSITED_EVENT_TOPIC)],
            "data": "0x00ab",
            "transactionHash": encode_hash(transaction_hash),
            "blockHash": encode_hash(block_hash),
            "blockNumber": "0x2a",
            "logIndex": "0x1",
            "removed": false
        });
        let decoded = decode_log(&value).unwrap();
        assert_eq!(decoded.address, address);
        assert_eq!(decoded.transaction_hash, Some(transaction_hash));
        assert_eq!(decoded.block_hash, Some(block_hash));
        assert_eq!(decoded.block_number, Some(42));
        assert_eq!(decoded.log_index, Some(1));
        assert_eq!(decoded.data, vec![0, 0xab]);

        let mut incomplete = value;
        incomplete
            .as_object_mut()
            .unwrap()
            .remove("transactionHash");
        assert_eq!(decode_log(&incomplete), Err(EvmRpcError::InvalidResponse));
    }
}
