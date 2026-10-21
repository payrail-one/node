use std::{env, ffi::OsString, net::IpAddr, path::PathBuf, time::Duration};

use ethereum_bridge_core::{
    EvmAddress, EvmAsset, EvmAssetBinding, EvmChainId, EvmHash, EvmObserverScope, EvmProviderId,
    MAX_FINALITY_PROVIDERS,
};
use ethereum_bridge_observer_lmdb::{EvmObserverStoreOptions, MINIMUM_MAP_SIZE};
use ethereum_bridge_rpc::RpcEvmFinalityProvider;
use ledger_core::AssetId;

use crate::error::ObserverServiceError;

const CHAIN_ID: &str = "PAYRAIL_ETH_CHAIN_ID";
const BRIDGE_ADDRESS: &str = "PAYRAIL_ETH_BRIDGE_ADDRESS";
const FIRST_BLOCK: &str = "PAYRAIL_ETH_FIRST_BLOCK";
const FIRST_PARENT_HASH: &str = "PAYRAIL_ETH_FIRST_PARENT_HASH";
const RPC_PROVIDERS: &str = "PAYRAIL_ETH_RPC_PROVIDERS";
const QUORUM: &str = "PAYRAIL_ETH_QUORUM";
const ASSETS: &str = "PAYRAIL_ETH_ASSETS";
const DATABASE_PATH: &str = "PAYRAIL_ETH_OBSERVER_DB";
const MAP_SIZE: &str = "PAYRAIL_ETH_OBSERVER_MAP_SIZE";
const POLL_MILLIS: &str = "PAYRAIL_ETH_POLL_MILLIS";
const GATEWAY_URL: &str = "PAYRAIL_BRIDGE_GATEWAY_URL";
const NETWORK_ID: &str = "PAYRAIL_BRIDGE_NETWORK_ID";
const OBSERVER_KEY: &str = "PAYRAIL_BRIDGE_OBSERVER_KEY_FILE";
const FINALITY_KEY: &str = "PAYRAIL_BRIDGE_FINALITY_KEY_FILE";
const SETTLEMENT_KEY: &str = "PAYRAIL_BRIDGE_SETTLEMENT_KEY_FILE";
const SETTLEMENT_CURSOR: &str = "PAYRAIL_BRIDGE_SETTLEMENT_CURSOR";

const DEFAULT_POLL_MILLIS: u64 = 3_000;
const MINIMUM_POLL_MILLIS: u64 = 100;
const MAXIMUM_POLL_MILLIS: u64 = 60_000;

#[derive(Debug)]
pub struct ObserverConfig {
    pub scope: EvmObserverScope,
    pub bindings: Vec<EvmAssetBinding>,
    pub providers: Vec<RpcEvmFinalityProvider>,
    pub minimum_agreement: usize,
    pub store_path: PathBuf,
    pub store_options: EvmObserverStoreOptions,
    pub poll_interval: Duration,
}

#[derive(Debug)]
pub struct SettlementConfig {
    pub gateway_url: reqwest::Url,
    pub network: ledger_core::NetworkId,
    pub observer_key: PathBuf,
    pub finality_key: PathBuf,
    pub settlement_key: PathBuf,
    pub cursor_path: PathBuf,
}

impl ObserverConfig {
    pub fn from_environment() -> Result<Self, ObserverServiceError> {
        let chain_id = EvmChainId::new(parse_decimal(CHAIN_ID, &required(CHAIN_ID)?)?)
            .ok_or(ObserverServiceError::InvalidConfiguration("zero chain ID"))?;
        let bridge_contract = EvmAddress::new(parse_fixed(&required(BRIDGE_ADDRESS)?)?);
        let first_block = parse_decimal(FIRST_BLOCK, &required(FIRST_BLOCK)?)?;
        let first_parent_hash = EvmHash::new(parse_fixed(&required(FIRST_PARENT_HASH)?)?);
        let scope =
            EvmObserverScope::new(chain_id, bridge_contract, first_block, first_parent_hash)
                .map_err(|_| {
                    ObserverServiceError::InvalidConfiguration("invalid observer scope")
                })?;
        let providers = parse_providers(chain_id, &required(RPC_PROVIDERS)?)?;
        let minimum_agreement = parse_usize(QUORUM, &required(QUORUM)?)?;
        if !(2..=providers.len()).contains(&minimum_agreement) {
            return Err(ObserverServiceError::InvalidConfiguration(
                "quorum must be between two and provider count",
            ));
        }
        let bindings = parse_bindings(scope, &required(ASSETS)?)?;
        if bindings.is_empty() {
            return Err(ObserverServiceError::InvalidConfiguration(
                "at least one asset binding is required",
            ));
        }
        let store_path = PathBuf::from(required_os(DATABASE_PATH)?);
        let map_size = optional(MAP_SIZE)?
            .map(|value| parse_usize(MAP_SIZE, &value))
            .transpose()?
            .unwrap_or(64 * MINIMUM_MAP_SIZE);
        let poll_millis = optional(POLL_MILLIS)?
            .map(|value| parse_decimal(POLL_MILLIS, &value))
            .transpose()?
            .unwrap_or(DEFAULT_POLL_MILLIS);
        if !(MINIMUM_POLL_MILLIS..=MAXIMUM_POLL_MILLIS).contains(&poll_millis) {
            return Err(ObserverServiceError::InvalidConfiguration(
                "poll interval outside 100..=60000 milliseconds",
            ));
        }
        Ok(Self {
            scope,
            bindings,
            providers,
            minimum_agreement,
            store_path,
            store_options: EvmObserverStoreOptions {
                map_size,
                ..EvmObserverStoreOptions::default()
            },
            poll_interval: Duration::from_millis(poll_millis),
        })
    }

    pub fn settlement() -> Result<SettlementConfig, ObserverServiceError> {
        let gateway_url = reqwest::Url::parse(&required(GATEWAY_URL)?)
            .map_err(|_| ObserverServiceError::InvalidConfiguration("invalid gateway URL"))?;
        if !is_allowed_gateway_origin(&gateway_url)
            || gateway_url.username() != ""
            || gateway_url.password().is_some()
            || gateway_url.path() != "/"
            || gateway_url.query().is_some()
            || gateway_url.fragment().is_some()
        {
            return Err(ObserverServiceError::InvalidConfiguration(
                "gateway URL must be an HTTPS or private-network HTTP origin",
            ));
        }
        Ok(SettlementConfig {
            gateway_url,
            network: ledger_core::NetworkId::new(parse_fixed(&required(NETWORK_ID)?)?),
            observer_key: PathBuf::from(required_os(OBSERVER_KEY)?),
            finality_key: PathBuf::from(required_os(FINALITY_KEY)?),
            settlement_key: PathBuf::from(required_os(SETTLEMENT_KEY)?),
            cursor_path: PathBuf::from(required_os(SETTLEMENT_CURSOR)?),
        })
    }
}

fn is_allowed_gateway_origin(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() != "http" {
        return false;
    }
    match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => address.is_private() || address.is_loopback(),
        Ok(IpAddr::V6(address)) => address.is_unique_local() || address.is_loopback(),
        Err(_) => false,
    }
}

fn parse_providers(
    chain_id: EvmChainId,
    value: &str,
) -> Result<Vec<RpcEvmFinalityProvider>, ObserverServiceError> {
    let entries = value.split(',').collect::<Vec<_>>();
    if !(2..=MAX_FINALITY_PROVIDERS).contains(&entries.len()) {
        return Err(ObserverServiceError::InvalidConfiguration(
            "RPC provider count outside 2..=8",
        ));
    }
    entries
        .into_iter()
        .map(|entry| {
            let (raw_id, endpoint) =
                entry
                    .split_once('=')
                    .ok_or(ObserverServiceError::InvalidConfiguration(
                        "invalid RPC provider entry",
                    ))?;
            let id = EvmProviderId::new(parse_fixed(raw_id)?);
            RpcEvmFinalityProvider::new(id, chain_id, endpoint)
                .map_err(|_| ObserverServiceError::RpcInitialization)
        })
        .collect()
}

fn parse_bindings(
    scope: EvmObserverScope,
    value: &str,
) -> Result<Vec<EvmAssetBinding>, ObserverServiceError> {
    value
        .split(',')
        .map(|entry| {
            let (raw_origin, raw_payrail) =
                entry
                    .split_once('=')
                    .ok_or(ObserverServiceError::InvalidConfiguration(
                        "invalid asset binding entry",
                    ))?;
            let origin_asset = if raw_origin == "native" {
                EvmAsset::Native
            } else {
                EvmAsset::Erc20(EvmAddress::new(parse_fixed(raw_origin)?))
            };
            EvmAssetBinding::new(
                scope.chain_id,
                scope.bridge_contract,
                origin_asset,
                AssetId::new(parse_fixed(raw_payrail)?),
            )
            .map_err(|_| ObserverServiceError::InvalidConfiguration("invalid asset binding"))
        })
        .collect()
}

fn required(name: &'static str) -> Result<String, ObserverServiceError> {
    required_os(name)?
        .into_string()
        .map_err(|_| ObserverServiceError::InvalidConfiguration("non-UTF-8 environment value"))
}

fn required_os(name: &'static str) -> Result<OsString, ObserverServiceError> {
    env::var_os(name).ok_or(ObserverServiceError::MissingConfiguration(name))
}

fn optional(name: &'static str) -> Result<Option<String>, ObserverServiceError> {
    env::var_os(name)
        .map(|value| {
            value.into_string().map_err(|_| {
                ObserverServiceError::InvalidConfiguration("non-UTF-8 environment value")
            })
        })
        .transpose()
}

fn parse_decimal(name: &'static str, value: &str) -> Result<u64, ObserverServiceError> {
    value
        .parse()
        .map_err(|_| ObserverServiceError::InvalidConfiguration(name))
}

fn parse_usize(name: &'static str, value: &str) -> Result<usize, ObserverServiceError> {
    value
        .parse()
        .map_err(|_| ObserverServiceError::InvalidConfiguration(name))
}

fn parse_fixed<const LENGTH: usize>(value: &str) -> Result<[u8; LENGTH], ObserverServiceError> {
    let digits = value
        .strip_prefix("0x")
        .ok_or(ObserverServiceError::InvalidConfiguration(
            "hex value requires 0x prefix",
        ))?;
    if digits.len() != LENGTH.saturating_mul(2) {
        return Err(ObserverServiceError::InvalidConfiguration(
            "wrong hex value length",
        ));
    }
    let mut output = [0_u8; LENGTH];
    for (destination, pair) in output.iter_mut().zip(digits.as_bytes().as_chunks::<2>().0) {
        let high = nibble(pair[0])?;
        let low = nibble(pair[1])?;
        *destination = high * 16 + low;
    }
    Ok(output)
}

fn nibble(value: u8) -> Result<u8, ObserverServiceError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(ObserverServiceError::InvalidConfiguration(
            "hex values must use lowercase digits",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_allowed_gateway_origin, parse_fixed};

    #[test]
    fn fixed_hex_is_exact_lowercase_and_prefixed() {
        assert_eq!(parse_fixed::<2>("0x00af").unwrap(), [0, 0xaf]);
        for invalid in ["00af", "0x0af", "0x00AF", "0x00ag"] {
            assert!(parse_fixed::<2>(invalid).is_err());
        }
    }

    #[test]
    fn settlement_gateway_requires_encrypted_or_private_origin() {
        for allowed in [
            "https://devnet.payrail.one",
            "http://127.0.0.1:18083",
            "http://192.168.9.69:18083",
            "http://[::1]:18083",
            "http://[fd00::1]:18083",
        ] {
            assert!(is_allowed_gateway_origin(&allowed.parse().unwrap()));
        }
        for rejected in [
            "http://devnet.payrail.one",
            "http://8.8.8.8",
            "ftp://192.168.9.69",
        ] {
            assert!(!is_allowed_gateway_origin(&rejected.parse().unwrap()));
        }
    }
}
