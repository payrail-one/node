use ledger_core::{
    AccountId, AccountStatus, AssetClass, AssetDefinition, AssetId, AssetStatus,
    BackingRequirement, ContractId, ContractRecord, ContractStateEntry, Ledger, LedgerSnapshot,
    NetworkId, SnapshotAccountStatus, SnapshotAsset, SnapshotBalance, SnapshotNonce,
};

use crate::{RuntimeError, decoder::Decoder};

const MAX_SYMBOL_BYTES: usize = 16;
const OPERATION_SEQUENCE_KEY: &[u8] = b"value";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerRow {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerStateRows {
    pub network: NetworkId,
    pub registry_authority: AccountId,
    pub assets: Vec<LedgerRow>,
    pub balances: Vec<LedgerRow>,
    pub nonces: Vec<LedgerRow>,
    pub operation_sequence: Vec<LedgerRow>,
    pub account_statuses: Vec<LedgerRow>,
    pub contracts: Vec<LedgerRow>,
    pub contract_state: Vec<LedgerRow>,
}

impl LedgerStateRows {
    /// Converts a validated canonical snapshot into normalized key/value rows.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot violates ledger invariants.
    pub fn from_snapshot(snapshot: &LedgerSnapshot) -> Result<Self, RuntimeError> {
        let canonical = Ledger::from_snapshot(snapshot.network, snapshot.clone())
            .map_err(RuntimeError::InvalidSnapshot)?
            .snapshot();
        Ok(Self {
            network: canonical.network,
            registry_authority: canonical.registry_authority,
            assets: canonical
                .assets
                .iter()
                .map(asset_row)
                .collect::<Result<_, _>>()?,
            balances: canonical.balances.iter().map(balance_row).collect(),
            nonces: canonical.nonces.iter().map(nonce_row).collect(),
            operation_sequence: vec![operation_sequence_row(canonical.next_operation_index)],
            account_statuses: canonical.account_statuses.iter().map(status_row).collect(),
            contracts: canonical
                .contracts
                .iter()
                .map(contract_row)
                .collect::<Result<_, _>>()?,
            contract_state: canonical
                .contract_state
                .iter()
                .map(contract_state_row)
                .collect(),
        })
    }

    /// Restores and validates a canonical snapshot from normalized rows.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed rows, a wrong network or broken invariants.
    pub fn into_snapshot(
        self,
        expected_network: NetworkId,
    ) -> Result<LedgerSnapshot, RuntimeError> {
        let snapshot = LedgerSnapshot {
            network: self.network,
            registry_authority: self.registry_authority,
            assets: self
                .assets
                .iter()
                .map(decode_asset_row)
                .collect::<Result<_, _>>()?,
            balances: self
                .balances
                .iter()
                .map(decode_balance_row)
                .collect::<Result<_, _>>()?,
            nonces: self
                .nonces
                .iter()
                .map(decode_nonce_row)
                .collect::<Result<_, _>>()?,
            next_operation_index: decode_operation_sequence(&self.operation_sequence)?,
            account_statuses: self
                .account_statuses
                .iter()
                .map(decode_status_row)
                .collect::<Result<_, _>>()?,
            contracts: self
                .contracts
                .iter()
                .map(decode_contract_row)
                .collect::<Result<_, _>>()?,
            contract_state: self
                .contract_state
                .iter()
                .map(decode_contract_state_row)
                .collect::<Result<_, _>>()?,
        };
        Ledger::from_snapshot(expected_network, snapshot)
            .map(|ledger| ledger.snapshot())
            .map_err(RuntimeError::InvalidSnapshot)
    }
}

fn contract_row(contract: &ContractRecord) -> Result<LedgerRow, RuntimeError> {
    let code_length =
        u16::try_from(contract.code.len()).map_err(|_| RuntimeError::LengthOverflow)?;
    let mut value = Vec::with_capacity(34 + contract.code.len());
    value.extend_from_slice(contract.owner.as_bytes());
    value.extend_from_slice(&code_length.to_be_bytes());
    value.extend_from_slice(&contract.code);
    Ok(LedgerRow {
        key: contract.id.as_bytes().to_vec(),
        value,
    })
}

fn decode_contract_row(row: &LedgerRow) -> Result<ContractRecord, RuntimeError> {
    let id = ContractId::new(array(&row.key)?);
    let mut decoder = Decoder::new(&row.value);
    let owner = AccountId::new(decoder.read_array()?);
    let code_length = usize::from(decoder.read_u16()?);
    let code = decoder.read_slice(code_length)?.to_vec();
    decoder.finish()?;
    Ok(ContractRecord { id, owner, code })
}

fn contract_state_row(entry: &ContractStateEntry) -> LedgerRow {
    let mut key = Vec::with_capacity(32 + entry.key.len());
    key.extend_from_slice(entry.contract.as_bytes());
    key.extend_from_slice(&entry.key);
    LedgerRow {
        key,
        value: entry.value.to_be_bytes().to_vec(),
    }
}

fn decode_contract_state_row(row: &LedgerRow) -> Result<ContractStateEntry, RuntimeError> {
    if row.key.len() <= 32 || row.key.len() > 64 {
        return Err(RuntimeError::UnsupportedValue);
    }
    Ok(ContractStateEntry {
        contract: ContractId::new(array(&row.key[..32])?),
        key: row.key[32..].to_vec(),
        value: u128::from_be_bytes(array(&row.value)?),
    })
}

fn asset_row(asset: &SnapshotAsset) -> Result<LedgerRow, RuntimeError> {
    let definition = &asset.definition;
    let symbol = definition.symbol.as_bytes();
    let symbol_length = u8::try_from(symbol.len()).map_err(|_| RuntimeError::LengthOverflow)?;
    if symbol.is_empty() || symbol.len() > MAX_SYMBOL_BYTES {
        return Err(RuntimeError::InvalidSnapshot(
            ledger_core::LedgerError::InvalidSymbol,
        ));
    }
    let mut value = Vec::new();
    value.push(symbol_length);
    value.extend_from_slice(symbol);
    value.push(definition.decimals);
    value.push(asset_class_value(definition.class));
    value.push(asset_status_value(definition.status));
    value.extend_from_slice(definition.issuer.as_bytes());
    value.extend_from_slice(definition.backing_authority.as_bytes());
    value.extend_from_slice(definition.freeze_authority.as_bytes());
    value.extend_from_slice(definition.treasury.as_bytes());
    match definition.max_supply {
        None => value.push(0),
        Some(maximum) => {
            value.push(1);
            value.extend_from_slice(&maximum.to_be_bytes());
        }
    }
    value.push(backing_value(definition.backing_requirement));
    value.extend_from_slice(&asset.supply.to_be_bytes());
    value.extend_from_slice(&asset.verified_backing.to_be_bytes());
    Ok(LedgerRow {
        key: definition.id.as_bytes().to_vec(),
        value,
    })
}

fn decode_asset_row(row: &LedgerRow) -> Result<SnapshotAsset, RuntimeError> {
    let id = AssetId::new(array(&row.key)?);
    let mut decoder = Decoder::new(&row.value);
    let symbol_length = usize::from(decoder.read_u8()?);
    if symbol_length == 0 || symbol_length > MAX_SYMBOL_BYTES {
        return Err(RuntimeError::UnsupportedValue);
    }
    let symbol = std::str::from_utf8(decoder.read_slice(symbol_length)?)
        .map_err(|_| RuntimeError::InvalidUtf8)?
        .to_owned();
    let decimals = decoder.read_u8()?;
    let class = decode_asset_class(decoder.read_u8()?)?;
    let status = decode_asset_status(decoder.read_u8()?)?;
    let issuer = AccountId::new(decoder.read_array()?);
    let backing_authority = AccountId::new(decoder.read_array()?);
    let freeze_authority = AccountId::new(decoder.read_array()?);
    let treasury = AccountId::new(decoder.read_array()?);
    let max_supply = match decoder.read_u8()? {
        0 => None,
        1 => Some(decoder.read_u128()?),
        _ => return Err(RuntimeError::UnsupportedValue),
    };
    let backing_requirement = decode_backing(decoder.read_u8()?)?;
    let supply = decoder.read_u128()?;
    let verified_backing = decoder.read_u128()?;
    decoder.finish()?;
    Ok(SnapshotAsset {
        definition: AssetDefinition {
            id,
            symbol,
            decimals,
            class,
            status,
            issuer,
            backing_authority,
            freeze_authority,
            treasury,
            max_supply,
            backing_requirement,
        },
        supply,
        verified_backing,
    })
}

fn balance_row(balance: &SnapshotBalance) -> LedgerRow {
    LedgerRow {
        key: joined_key(balance.asset.as_bytes(), balance.account.as_bytes()),
        value: balance.amount.to_be_bytes().to_vec(),
    }
}

fn decode_balance_row(row: &LedgerRow) -> Result<SnapshotBalance, RuntimeError> {
    Ok(SnapshotBalance {
        asset: AssetId::new(array(slice(&row.key, 0, 32)?)?),
        account: AccountId::new(array(slice(&row.key, 32, 32)?)?),
        amount: u128::from_be_bytes(array(&row.value)?),
    })
}

fn nonce_row(nonce: &SnapshotNonce) -> LedgerRow {
    LedgerRow {
        key: nonce.account.as_bytes().to_vec(),
        value: nonce.nonce.to_be_bytes().to_vec(),
    }
}

fn decode_nonce_row(row: &LedgerRow) -> Result<SnapshotNonce, RuntimeError> {
    Ok(SnapshotNonce {
        account: AccountId::new(array(&row.key)?),
        nonce: u64::from_be_bytes(array(&row.value)?),
    })
}

fn operation_sequence_row(next_operation_index: u64) -> LedgerRow {
    LedgerRow {
        key: OPERATION_SEQUENCE_KEY.to_vec(),
        value: next_operation_index.to_be_bytes().to_vec(),
    }
}

fn decode_operation_sequence(rows: &[LedgerRow]) -> Result<u64, RuntimeError> {
    let [row] = rows else {
        return Err(RuntimeError::UnsupportedValue);
    };
    if row.key != OPERATION_SEQUENCE_KEY {
        return Err(RuntimeError::UnsupportedValue);
    }
    Ok(u64::from_be_bytes(array(&row.value)?))
}

fn status_row(status: &SnapshotAccountStatus) -> LedgerRow {
    LedgerRow {
        key: joined_key(status.asset.as_bytes(), status.account.as_bytes()),
        value: vec![account_status_value(status.status)],
    }
}

fn decode_status_row(row: &LedgerRow) -> Result<SnapshotAccountStatus, RuntimeError> {
    if row.value.len() != 1 {
        return Err(RuntimeError::UnexpectedEnd);
    }
    Ok(SnapshotAccountStatus {
        asset: AssetId::new(array(slice(&row.key, 0, 32)?)?),
        account: AccountId::new(array(slice(&row.key, 32, 32)?)?),
        status: decode_account_status(row.value[0])?,
    })
}

fn joined_key(left: &[u8; 32], right: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(64);
    key.extend_from_slice(left);
    key.extend_from_slice(right);
    key
}

fn slice(input: &[u8], offset: usize, length: usize) -> Result<&[u8], RuntimeError> {
    input
        .get(offset..offset.saturating_add(length))
        .ok_or(RuntimeError::UnexpectedEnd)
}

fn array<const SIZE: usize>(input: &[u8]) -> Result<[u8; SIZE], RuntimeError> {
    input.try_into().map_err(|_| RuntimeError::UnexpectedEnd)
}

const fn asset_class_value(value: AssetClass) -> u8 {
    match value {
        AssetClass::NetworkNative => 0,
        AssetClass::Sovereign => 1,
        AssetClass::IssuerRegulated => 2,
        AssetClass::ExternalRepresentation => 3,
        AssetClass::Application => 4,
    }
}

fn decode_asset_class(value: u8) -> Result<AssetClass, RuntimeError> {
    [
        AssetClass::NetworkNative,
        AssetClass::Sovereign,
        AssetClass::IssuerRegulated,
        AssetClass::ExternalRepresentation,
        AssetClass::Application,
    ]
    .get(usize::from(value))
    .copied()
    .ok_or(RuntimeError::UnsupportedValue)
}

const fn asset_status_value(value: AssetStatus) -> u8 {
    match value {
        AssetStatus::Proposed => 0,
        AssetStatus::Test => 1,
        AssetStatus::Active => 2,
        AssetStatus::WithdrawOnly => 3,
        AssetStatus::Suspended => 4,
        AssetStatus::Retired => 5,
    }
}

fn decode_asset_status(value: u8) -> Result<AssetStatus, RuntimeError> {
    [
        AssetStatus::Proposed,
        AssetStatus::Test,
        AssetStatus::Active,
        AssetStatus::WithdrawOnly,
        AssetStatus::Suspended,
        AssetStatus::Retired,
    ]
    .get(usize::from(value))
    .copied()
    .ok_or(RuntimeError::UnsupportedValue)
}

const fn backing_value(value: BackingRequirement) -> u8 {
    match value {
        BackingRequirement::None => 0,
        BackingRequirement::VerifiedOneToOne => 1,
    }
}

fn decode_backing(value: u8) -> Result<BackingRequirement, RuntimeError> {
    match value {
        0 => Ok(BackingRequirement::None),
        1 => Ok(BackingRequirement::VerifiedOneToOne),
        _ => Err(RuntimeError::UnsupportedValue),
    }
}

const fn account_status_value(value: AccountStatus) -> u8 {
    match value {
        AccountStatus::Active => 0,
        AccountStatus::SendOnly => 1,
        AccountStatus::ReceiveOnly => 2,
        AccountStatus::Frozen => 3,
    }
}

fn decode_account_status(value: u8) -> Result<AccountStatus, RuntimeError> {
    [
        AccountStatus::Active,
        AccountStatus::SendOnly,
        AccountStatus::ReceiveOnly,
        AccountStatus::Frozen,
    ]
    .get(usize::from(value))
    .copied()
    .ok_or(RuntimeError::UnsupportedValue)
}
