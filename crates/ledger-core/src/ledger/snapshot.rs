use std::collections::BTreeMap;

use crate::{
    AccountId, AccountStatus, AssetDefinition, AssetId, AssetState, AssetStatus,
    BackingRequirement, Balance, ContractId, ContractRecord, ContractStateEntry, LedgerError,
    NetworkId, Nonce, contract::validate_code,
};

use super::{Ledger, validate_definition};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotAsset {
    pub definition: AssetDefinition,
    pub supply: Balance,
    pub verified_backing: Balance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotBalance {
    pub asset: AssetId,
    pub account: AccountId,
    pub amount: Balance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotNonce {
    pub account: AccountId,
    pub nonce: Nonce,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotAccountStatus {
    pub asset: AssetId,
    pub account: AccountId,
    pub status: AccountStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerSnapshot {
    pub network: NetworkId,
    pub registry_authority: AccountId,
    pub assets: Vec<SnapshotAsset>,
    pub balances: Vec<SnapshotBalance>,
    pub nonces: Vec<SnapshotNonce>,
    pub next_operation_index: u64,
    pub account_statuses: Vec<SnapshotAccountStatus>,
    pub contracts: Vec<ContractRecord>,
    pub contract_state: Vec<ContractStateEntry>,
}

impl Ledger {
    /// Exports consensus state in canonical key order.
    ///
    /// Events are intentionally excluded because block/event history belongs
    /// to the append-only chain store rather than the current state snapshot.
    #[must_use]
    pub fn snapshot(&self) -> LedgerSnapshot {
        LedgerSnapshot {
            network: self.network,
            registry_authority: self.registry_authority,
            assets: self
                .assets
                .values()
                .map(|state| SnapshotAsset {
                    definition: state.definition.clone(),
                    supply: state.supply,
                    verified_backing: state.verified_backing,
                })
                .collect(),
            balances: self
                .balances
                .iter()
                .map(|((asset, account), amount)| SnapshotBalance {
                    asset: *asset,
                    account: *account,
                    amount: *amount,
                })
                .collect(),
            nonces: self
                .nonces
                .iter()
                .map(|(account, nonce)| SnapshotNonce {
                    account: *account,
                    nonce: *nonce,
                })
                .collect(),
            next_operation_index: self.next_operation_index,
            account_statuses: self
                .account_statuses
                .iter()
                .map(|((asset, account), status)| SnapshotAccountStatus {
                    asset: *asset,
                    account: *account,
                    status: *status,
                })
                .collect(),
            contracts: self.contracts.values().cloned().collect(),
            contract_state: self
                .contract_state
                .iter()
                .map(|((contract, key), value)| ContractStateEntry {
                    contract: *contract,
                    key: key.clone(),
                    value: *value,
                })
                .collect(),
        }
    }

    /// Restores state only after validating canonical order and all monetary,
    /// backing, account-policy and replay-protection invariants.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot is non-canonical, references an
    /// unknown asset, or violates supply, backing, nonce, operation-sequence
    /// or policy invariants.
    pub fn from_snapshot(
        expected_network: NetworkId,
        snapshot: LedgerSnapshot,
    ) -> Result<Self, LedgerError> {
        if snapshot.network != expected_network {
            return Err(LedgerError::WrongNetwork);
        }
        validate_canonical_order(&snapshot)?;
        let assets = restore_assets(snapshot.assets)?;
        let balances = restore_balances(&assets, snapshot.balances)?;
        validate_supplies(&assets, &balances)?;
        let nonces = restore_nonces(snapshot.nonces);
        validate_operation_sequence(&nonces, snapshot.next_operation_index)?;
        let account_statuses = restore_account_statuses(&assets, snapshot.account_statuses)?;
        let contracts = restore_contracts(snapshot.contracts)?;
        let contract_state = restore_contract_state(&contracts, snapshot.contract_state)?;

        Ok(Self {
            network: snapshot.network,
            registry_authority: snapshot.registry_authority,
            assets,
            balances,
            nonces,
            next_operation_index: snapshot.next_operation_index,
            account_statuses,
            contracts,
            contract_state,
            events: Vec::new(),
        })
    }
}

fn validate_canonical_order(snapshot: &LedgerSnapshot) -> Result<(), LedgerError> {
    let assets_ok = snapshot
        .assets
        .windows(2)
        .all(|pair| pair[0].definition.id < pair[1].definition.id);
    let balances_ok = snapshot
        .balances
        .windows(2)
        .all(|pair| (pair[0].asset, pair[0].account) < (pair[1].asset, pair[1].account));
    let nonces_ok = snapshot
        .nonces
        .windows(2)
        .all(|pair| pair[0].account < pair[1].account);
    let statuses_ok = snapshot
        .account_statuses
        .windows(2)
        .all(|pair| (pair[0].asset, pair[0].account) < (pair[1].asset, pair[1].account));
    let contracts_ok = snapshot
        .contracts
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id);
    let contract_state_ok = snapshot
        .contract_state
        .windows(2)
        .all(|pair| (pair[0].contract, &pair[0].key) < (pair[1].contract, &pair[1].key));

    if assets_ok && balances_ok && nonces_ok && statuses_ok && contracts_ok && contract_state_ok {
        Ok(())
    } else {
        Err(LedgerError::NonCanonicalSnapshot)
    }
}

fn restore_contracts(
    contracts: Vec<ContractRecord>,
) -> Result<BTreeMap<ContractId, ContractRecord>, LedgerError> {
    let mut restored = BTreeMap::new();
    for contract in contracts {
        validate_code(&contract.code)?;
        restored.insert(contract.id, contract);
    }
    Ok(restored)
}

fn restore_contract_state(
    contracts: &BTreeMap<ContractId, ContractRecord>,
    state: Vec<ContractStateEntry>,
) -> Result<BTreeMap<(ContractId, Vec<u8>), u128>, LedgerError> {
    let mut restored = BTreeMap::new();
    for entry in state {
        if !contracts.contains_key(&entry.contract)
            || entry.key.is_empty()
            || entry.key.len() > 32
            || entry.value == 0
        {
            return Err(LedgerError::SnapshotPolicyViolation);
        }
        restored.insert((entry.contract, entry.key), entry.value);
    }
    Ok(restored)
}

fn restore_assets(
    assets: Vec<SnapshotAsset>,
) -> Result<BTreeMap<AssetId, AssetState>, LedgerError> {
    let mut restored = BTreeMap::new();
    for asset in assets {
        validate_definition(&asset.definition)?;
        if asset
            .definition
            .max_supply
            .is_some_and(|maximum| asset.supply > maximum)
        {
            return Err(LedgerError::SnapshotPolicyViolation);
        }
        match asset.definition.backing_requirement {
            BackingRequirement::None if asset.verified_backing != 0 => {
                return Err(LedgerError::SnapshotPolicyViolation);
            }
            BackingRequirement::VerifiedOneToOne
                if asset.verified_backing < asset.supply
                    && asset.definition.status != AssetStatus::Suspended =>
            {
                return Err(LedgerError::SnapshotPolicyViolation);
            }
            _ => {}
        }
        restored.insert(
            asset.definition.id,
            AssetState {
                definition: asset.definition,
                supply: asset.supply,
                verified_backing: asset.verified_backing,
            },
        );
    }
    Ok(restored)
}

fn restore_balances(
    assets: &BTreeMap<AssetId, AssetState>,
    balances: Vec<SnapshotBalance>,
) -> Result<BTreeMap<(AssetId, AccountId), Balance>, LedgerError> {
    let mut restored = BTreeMap::new();
    for balance in balances {
        if balance.amount == 0 {
            return Err(LedgerError::SnapshotPolicyViolation);
        }
        if !assets.contains_key(&balance.asset) {
            return Err(LedgerError::SnapshotUnknownAsset);
        }
        restored.insert((balance.asset, balance.account), balance.amount);
    }
    Ok(restored)
}

fn validate_supplies(
    assets: &BTreeMap<AssetId, AssetState>,
    balances: &BTreeMap<(AssetId, AccountId), Balance>,
) -> Result<(), LedgerError> {
    let mut totals = BTreeMap::<AssetId, Balance>::new();
    for ((asset, _), amount) in balances {
        let total = totals.get(asset).copied().unwrap_or(0);
        totals.insert(
            *asset,
            total
                .checked_add(*amount)
                .ok_or(LedgerError::ArithmeticOverflow)?,
        );
    }
    for (asset, state) in assets {
        if totals.get(asset).copied().unwrap_or(0) != state.supply {
            return Err(LedgerError::SnapshotSupplyMismatch);
        }
    }
    Ok(())
}

fn restore_nonces(nonces: Vec<SnapshotNonce>) -> BTreeMap<AccountId, Nonce> {
    nonces
        .into_iter()
        .map(|entry| (entry.account, entry.nonce))
        .collect()
}

fn validate_operation_sequence(
    nonces: &BTreeMap<AccountId, Nonce>,
    next_operation_index: u64,
) -> Result<(), LedgerError> {
    if nonces.values().any(|nonce| *nonce == 0) {
        return Err(LedgerError::SnapshotOperationMismatch);
    }
    let operation_count = nonces.values().try_fold(0_u64, |total, nonce| {
        total
            .checked_add(*nonce)
            .ok_or(LedgerError::ArithmeticOverflow)
    })?;
    if operation_count != next_operation_index {
        return Err(LedgerError::SnapshotOperationMismatch);
    }
    Ok(())
}

fn restore_account_statuses(
    assets: &BTreeMap<AssetId, AssetState>,
    statuses: Vec<SnapshotAccountStatus>,
) -> Result<BTreeMap<(AssetId, AccountId), AccountStatus>, LedgerError> {
    let mut restored = BTreeMap::new();
    for entry in statuses {
        if !assets.contains_key(&entry.asset) {
            return Err(LedgerError::SnapshotUnknownAsset);
        }
        if entry.status == AccountStatus::Active {
            return Err(LedgerError::SnapshotPolicyViolation);
        }
        restored.insert((entry.asset, entry.account), entry.status);
    }
    Ok(restored)
}
