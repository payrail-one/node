use std::{net::SocketAddr, path::PathBuf, time::Duration};

use reqwest::Url;

use crate::error::NodeError;

pub struct NodeConfig {
    pub bind: SocketAddr,
    pub data_directory: PathBuf,
    pub upstreams: Vec<Url>,
    pub sync_interval: Duration,
}

impl NodeConfig {
    /// Reads and validates public-node runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed bind addresses, upstream origins or sync intervals.
    pub fn from_environment() -> Result<Self, NodeError> {
        let bind = environment("PAYRAIL_NODE_BIND", "127.0.0.1:18080")
            .parse::<SocketAddr>()
            .map_err(|_| NodeError::Configuration("PAYRAIL_NODE_BIND is invalid"))?;
        let data_directory = PathBuf::from(environment("PAYRAIL_NODE_DATA_DIR", ".payrail/node"));
        let upstreams = parse_upstreams(&environment(
            "PAYRAIL_NODE_UPSTREAMS",
            "https://devnet.payrail.one",
        ))?;
        let seconds = environment("PAYRAIL_NODE_SYNC_INTERVAL_SECONDS", "2")
            .parse::<u64>()
            .map_err(|_| {
                NodeError::Configuration("PAYRAIL_NODE_SYNC_INTERVAL_SECONDS is invalid")
            })?;
        if !(1..=300).contains(&seconds) {
            return Err(NodeError::Configuration(
                "PAYRAIL_NODE_SYNC_INTERVAL_SECONDS must be between 1 and 300",
            ));
        }
        Ok(Self {
            bind,
            data_directory,
            upstreams,
            sync_interval: Duration::from_secs(seconds),
        })
    }
}

fn environment(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn parse_upstreams(value: &str) -> Result<Vec<Url>, NodeError> {
    let mut upstreams = Vec::new();
    for raw in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let mut url = Url::parse(raw).map_err(|_| {
            NodeError::Configuration("PAYRAIL_NODE_UPSTREAMS contains an invalid URL")
        })?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(NodeError::Configuration(
                "upstream URLs must be credential-free HTTP(S) origins",
            ));
        }
        url.set_path("/");
        upstreams.push(url);
    }
    if upstreams.is_empty() || upstreams.len() > 8 {
        return Err(NodeError::Configuration(
            "configure between one and eight upstream origins",
        ));
    }
    Ok(upstreams)
}
