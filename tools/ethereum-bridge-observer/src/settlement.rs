use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use ed25519_dalek::{Signer, SigningKey};
use ethereum_bridge_observer_lmdb::{LmdbEvmObserverStore, MAX_DEPOSIT_PAGE_SIZE};
use external_asset_core::{DepositId, SettlementReference};
use external_asset_runtime_core::{
    BridgeAuthorizationRole, DepositSettlementOperation, ExternalSettlementBlockCodec,
    ExternalSettlementOperation,
};
use ledger_core::{AccountId, Authorization, SignatureBytes};
use reqwest::{StatusCode, blocking::Client, redirect::Policy};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    config::{ObserverConfig, SettlementConfig},
    error::ObserverServiceError,
};

const REFERENCE_DOMAIN: &[u8] = b"payrail.ethereum.deposit.settlement.v1";

#[derive(Serialize)]
struct BridgeRequest {
    payload: String,
}

pub fn settle_deposits(store: &LmdbEvmObserverStore) -> Result<usize, ObserverServiceError> {
    let config = ObserverConfig::settlement()?;
    let keys = AuthorityKeys::read(&config)?;
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .map_err(|_| ObserverServiceError::SettlementConfiguration)?;
    let mut cursor = read_cursor(&config.cursor_path)?;
    let mut settled = 0_usize;
    loop {
        let deposits = store.deposits_after(cursor, MAX_DEPOSIT_PAGE_SIZE)?;
        if deposits.is_empty() {
            return Ok(settled);
        }
        for deposit in deposits {
            let payload = signed_payload(config.network, &deposit, &keys)?;
            submit(&client, &config.gateway_url, &payload)?;
            write_cursor(&config.cursor_path, deposit.id)?;
            cursor = Some(deposit.id);
            settled = settled
                .checked_add(1)
                .ok_or(ObserverServiceError::SettlementCursor)?;
        }
    }
}

struct AuthorityKeys {
    observer: SigningKey,
    finality: SigningKey,
    settlement: SigningKey,
}

impl AuthorityKeys {
    fn read(config: &SettlementConfig) -> Result<Self, ObserverServiceError> {
        Ok(Self {
            observer: read_key(&config.observer_key)?,
            finality: read_key(&config.finality_key)?,
            settlement: read_key(&config.settlement_key)?,
        })
    }
}

fn signed_payload(
    network: ledger_core::NetworkId,
    deposit: &ethereum_bridge_observer_lmdb::StoredEvmDeposit,
    keys: &AuthorityKeys,
) -> Result<Vec<u8>, ObserverServiceError> {
    let blank = Authorization {
        signer: AccountId::new([0; 32]),
        signature: SignatureBytes::new([0; 64]),
    };
    let mut reference_hasher = Sha256::new();
    reference_hasher.update(REFERENCE_DOMAIN);
    reference_hasher.update(deposit.id.as_bytes());
    let mut operation = DepositSettlementOperation {
        network,
        claim: deposit.claim,
        evidence: deposit.evidence,
        reference: SettlementReference::new(reference_hasher.finalize().into()),
        observer_authorization: blank,
        finality_authorization: blank,
        settlement_authorization: blank,
    };
    operation.observer_authorization = sign(
        &keys.observer,
        &operation.authorization_message(BridgeAuthorizationRole::Observer),
    );
    operation.finality_authorization = sign(
        &keys.finality,
        &operation.authorization_message(BridgeAuthorizationRole::Finality),
    );
    operation.settlement_authorization = sign(
        &keys.settlement,
        &operation.authorization_message(BridgeAuthorizationRole::Settlement),
    );
    ExternalSettlementBlockCodec::encode(ExternalSettlementOperation::Deposit(operation))
        .map_err(|_| ObserverServiceError::SettlementCodec)
}

fn sign(key: &SigningKey, message: &[u8]) -> Authorization {
    Authorization {
        signer: AccountId::new(key.verifying_key().to_bytes()),
        signature: SignatureBytes::new(key.sign(message).to_bytes()),
    }
}

fn submit(
    client: &Client,
    origin: &reqwest::Url,
    payload: &[u8],
) -> Result<(), ObserverServiceError> {
    let endpoint = origin
        .join("/api/bridge/operations")
        .map_err(|_| ObserverServiceError::SettlementConfiguration)?;
    let response = client
        .post(endpoint)
        .json(&BridgeRequest {
            payload: encode_hex(payload),
        })
        .send()
        .map_err(|_| ObserverServiceError::SettlementRequest)?;
    if response.status() == StatusCode::OK {
        Ok(())
    } else {
        Err(ObserverServiceError::SettlementRejected(
            response.status().as_u16(),
        ))
    }
}

fn read_key(path: &Path) -> Result<SigningKey, ObserverServiceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ObserverServiceError::SettlementKey)?;
    if !metadata.file_type().is_file() || metadata.len() != 32 {
        return Err(ObserverServiceError::SettlementKey);
    }
    let seed: [u8; 32] = fs::read(path)
        .map_err(|_| ObserverServiceError::SettlementKey)?
        .try_into()
        .map_err(|_| ObserverServiceError::SettlementKey)?;
    Ok(SigningKey::from_bytes(&seed))
}

fn read_cursor(path: &Path) -> Result<Option<DepositId>, ObserverServiceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.len() != 32 {
                return Err(ObserverServiceError::SettlementCursor);
            }
            fs::read(path)
                .map_err(|_| ObserverServiceError::SettlementCursor)?
                .try_into()
                .map(DepositId::new)
                .map(Some)
                .map_err(|_| ObserverServiceError::SettlementCursor)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ObserverServiceError::SettlementCursor),
    }
}

fn write_cursor(path: &Path, cursor: DepositId) -> Result<(), ObserverServiceError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(ObserverServiceError::SettlementCursor)?;
    let parent_metadata =
        fs::symlink_metadata(parent).map_err(|_| ObserverServiceError::SettlementCursor)?;
    if !parent_metadata.file_type().is_dir() {
        return Err(ObserverServiceError::SettlementCursor);
    }
    let temporary = temporary_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| ObserverServiceError::SettlementCursor)?;
    file.write_all(cursor.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| ObserverServiceError::SettlementCursor)?;
    fs::rename(&temporary, path).map_err(|_| ObserverServiceError::SettlementCursor)?;
    FileSync::sync(parent)
}

struct FileSync;

impl FileSync {
    fn sync(path: &Path) -> Result<(), ObserverServiceError> {
        fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| ObserverServiceError::SettlementCursor)
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    PathBuf::from(name)
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::encode_hex;

    #[test]
    fn payload_hex_is_lowercase_and_unprefixed() {
        assert_eq!(encode_hex(&[0, 0xaf, 0xff]), "00afff");
    }
}
