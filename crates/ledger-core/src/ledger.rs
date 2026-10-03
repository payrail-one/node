use std::collections::BTreeMap;

mod account;
mod contract;
mod expiry;
mod payment;
pub mod snapshot;

use crate::{
    AccountId, AccountStatus, AssetAudit, AssetDefinition, AssetId, AssetState, AssetStatus,
    BackingRequirement, Balance, Event, GenesisConfig, LedgerError, NetworkId, Nonce,
};

#[derive(Debug)]
pub struct Ledger {
    network: NetworkId,
    registry_authority: AccountId,
    assets: BTreeMap<AssetId, AssetState>,
    balances: BTreeMap<(AssetId, AccountId), Balance>,
    nonces: BTreeMap<AccountId, Nonce>,
    next_operation_index: u64,
    account_statuses: BTreeMap<(AssetId, AccountId), AccountStatus>,
    contracts: BTreeMap<crate::ContractId, crate::ContractRecord>,
    contract_state: BTreeMap<(crate::ContractId, Vec<u8>), u128>,
    events: Vec<Event>,
}

impl Ledger {
    #[must_use]
    pub fn new(network: NetworkId, registry_authority: AccountId) -> Self {
        Self {
            network,
            registry_authority,
            assets: BTreeMap::new(),
            balances: BTreeMap::new(),
            nonces: BTreeMap::new(),
            next_operation_index: 0,
            account_statuses: BTreeMap::new(),
            contracts: BTreeMap::new(),
            contract_state: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// Builds a ledger from a deterministic, canonically ordered genesis.
    ///
    /// # Errors
    ///
    /// Returns an error when ordering, asset metadata, backing, supply caps or
    /// initial balances violate ledger invariants.
    pub fn from_genesis(config: GenesisConfig) -> Result<Self, LedgerError> {
        crate::genesis::build(config)
    }

    /// Adds an asset to the canonical registry.
    ///
    /// # Errors
    ///
    /// Returns an error when the caller is not the registry authority, the
    /// identifier already exists, or symbol/decimal metadata is invalid.
    pub fn register_asset(
        &mut self,
        origin: AccountId,
        definition: AssetDefinition,
    ) -> Result<(), LedgerError> {
        if origin != self.registry_authority {
            return Err(LedgerError::Unauthorized);
        }
        if self.assets.contains_key(&definition.id) {
            return Err(LedgerError::AssetAlreadyExists);
        }
        validate_definition(&definition)?;

        let asset = definition.id;
        self.assets.insert(
            asset,
            AssetState {
                definition,
                supply: 0,
                verified_backing: 0,
            },
        );
        self.events.push(Event::AssetRegistered { asset });
        Ok(())
    }

    /// Changes the lifecycle status of a registered asset.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset does not exist, the caller is not the
    /// registry authority, or backing is in deficit.
    pub fn set_status(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        status: AssetStatus,
    ) -> Result<(), LedgerError> {
        if origin != self.registry_authority {
            return Err(LedgerError::Unauthorized);
        }
        let state = self.asset_mut(asset)?;
        if status_enables_operations(status) && has_backing_deficit(state) {
            return Err(LedgerError::BackingDeficit);
        }
        state.definition.status = status;
        self.events.push(Event::StatusChanged { asset, status });
        Ok(())
    }

    /// Records externally verified backing and suspends a deficit asset.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown asset, an unauthorized backing
    /// authority, or an asset that does not use verified one-to-one backing.
    pub fn observe_backing(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        amount: Balance,
    ) -> Result<(), LedgerError> {
        let state = self.asset_mut(asset)?;
        if origin != state.definition.backing_authority {
            return Err(LedgerError::Unauthorized);
        }
        if state.definition.backing_requirement != BackingRequirement::VerifiedOneToOne {
            return Err(LedgerError::BackingNotApplicable);
        }

        state.verified_backing = amount;
        let deficit = amount < state.supply;
        if deficit {
            state.definition.status = AssetStatus::Suspended;
        }
        self.events.push(Event::BackingObserved {
            asset,
            amount,
            deficit,
        });
        Ok(())
    }

    /// Issues units under the asset's configured issuance and backing policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero value, unknown or inactive assets,
    /// unauthorized issuance, arithmetic overflow, a supply-cap breach, or
    /// insufficient verified backing.
    pub fn mint(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        to: AccountId,
        amount: Balance,
    ) -> Result<(), LedgerError> {
        if amount == 0 {
            return Err(LedgerError::InvalidAmount);
        }

        let state = self.asset(asset)?;
        validate_mint(state, origin, amount)?;
        let recipient_status = self.account_status(asset, to);
        if !recipient_status.allows_receive() {
            return if recipient_status == AccountStatus::Frozen {
                Err(LedgerError::AccountFrozen)
            } else {
                Err(LedgerError::AccountCannotReceive)
            };
        }
        let new_supply = checked_add(state.supply, amount)?;
        let new_balance = checked_add(self.balance(asset, to), amount)?;

        self.asset_mut(asset)?.supply = new_supply;
        self.set_balance(asset, to, new_balance);
        self.events.push(Event::Minted { asset, to, amount });
        Ok(())
    }

    /// Burns units from an account and reduces declared supply atomically.
    ///
    /// # Errors
    ///
    /// Returns an error for zero value, an unknown asset, unauthorized
    /// issuance authority, insufficient balance, or arithmetic failure.
    pub fn burn(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        from: AccountId,
        amount: Balance,
    ) -> Result<(), LedgerError> {
        if amount == 0 {
            return Err(LedgerError::InvalidAmount);
        }
        let state = self.asset(asset)?;
        if origin != state.definition.issuer {
            return Err(LedgerError::Unauthorized);
        }

        let new_balance = self
            .balance(asset, from)
            .checked_sub(amount)
            .ok_or(LedgerError::InsufficientBalance)?;
        let new_supply = state
            .supply
            .checked_sub(amount)
            .ok_or(LedgerError::ArithmeticOverflow)?;

        self.asset_mut(asset)?.supply = new_supply;
        self.set_balance(asset, from, new_balance);
        self.events.push(Event::Burned {
            asset,
            from,
            amount,
        });
        Ok(())
    }

    #[must_use]
    pub fn balance(&self, asset: AssetId, account: AccountId) -> Balance {
        self.balances.get(&(asset, account)).copied().unwrap_or(0)
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub fn nonce(&self, account: AccountId) -> Nonce {
        self.nonces.get(&account).copied().unwrap_or(0)
    }

    #[must_use]
    pub const fn next_operation_index(&self) -> u64 {
        self.next_operation_index
    }

    #[must_use]
    pub fn total_supply(&self, asset: AssetId) -> Option<Balance> {
        self.assets.get(&asset).map(|state| state.supply)
    }

    #[must_use]
    pub fn definition(&self, asset: AssetId) -> Option<&AssetDefinition> {
        self.assets.get(&asset).map(|state| &state.definition)
    }

    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Reconciles declared supply with balances and verified backing.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset is unknown or summing balances would
    /// overflow the balance type.
    pub fn audit_asset(&self, asset: AssetId) -> Result<AssetAudit, LedgerError> {
        let state = self.asset(asset)?;
        let summed_balances = self
            .balances
            .iter()
            .filter(|((balance_asset, _), _)| *balance_asset == asset)
            .try_fold(0_u128, |total, (_, balance)| total.checked_add(*balance))
            .ok_or(LedgerError::ArithmeticOverflow)?;

        Ok(AssetAudit {
            declared_supply: state.supply,
            summed_balances,
            verified_backing: state.verified_backing,
            supply_matches_balances: state.supply == summed_balances,
            backing_covers_supply: !has_backing_deficit(state),
            status: state.definition.status,
        })
    }

    fn asset(&self, asset: AssetId) -> Result<&AssetState, LedgerError> {
        self.assets.get(&asset).ok_or(LedgerError::AssetNotFound)
    }

    fn asset_mut(&mut self, asset: AssetId) -> Result<&mut AssetState, LedgerError> {
        self.assets
            .get_mut(&asset)
            .ok_or(LedgerError::AssetNotFound)
    }

    fn set_balance(&mut self, asset: AssetId, account: AccountId, balance: Balance) {
        if balance == 0 {
            self.balances.remove(&(asset, account));
        } else {
            self.balances.insert((asset, account), balance);
        }
    }
}

fn validate_definition(definition: &AssetDefinition) -> Result<(), LedgerError> {
    let symbol_is_valid = !definition.symbol.is_empty()
        && definition.symbol.len() <= 16
        && definition
            .symbol
            .bytes()
            .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit());
    if !symbol_is_valid {
        return Err(LedgerError::InvalidSymbol);
    }
    if definition.decimals > 18 {
        return Err(LedgerError::InvalidDecimals);
    }
    Ok(())
}

fn validate_mint(
    state: &AssetState,
    origin: AccountId,
    amount: Balance,
) -> Result<(), LedgerError> {
    if origin != state.definition.issuer {
        return Err(LedgerError::Unauthorized);
    }
    if !state.definition.status.allows_mint() {
        return Err(LedgerError::AssetNotMintable);
    }

    let new_supply = checked_add(state.supply, amount)?;
    if state
        .definition
        .max_supply
        .is_some_and(|maximum| new_supply > maximum)
    {
        return Err(LedgerError::SupplyCapExceeded);
    }
    if state.definition.backing_requirement == BackingRequirement::VerifiedOneToOne
        && new_supply > state.verified_backing
    {
        return Err(LedgerError::BackingExceeded);
    }
    Ok(())
}

fn status_enables_operations(status: AssetStatus) -> bool {
    status.allows_transfer() || status.allows_mint()
}

fn has_backing_deficit(state: &AssetState) -> bool {
    state.definition.backing_requirement == BackingRequirement::VerifiedOneToOne
        && state.verified_backing < state.supply
}

fn checked_add(left: Balance, right: Balance) -> Result<Balance, LedgerError> {
    left.checked_add(right)
        .ok_or(LedgerError::ArithmeticOverflow)
}
