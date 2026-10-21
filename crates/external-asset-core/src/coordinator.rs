use std::collections::BTreeMap;

pub mod snapshot;

use ledger_core::{AccountId, AssetId, Balance};

use crate::{
    AssetSettlementAudit, ConnectorStatus, DepositId, DepositRecord, DepositStatus,
    ExternalAssetConfig, ExternalAssetError, ExternalAssetEvent, FinalityEvidence, MintInstruction,
    OriginAssetId, OriginDeposit, OriginNetworkId, OriginReleaseReference, OriginTransactionId,
    ReleaseInstruction, SettlementReference, WithdrawalId, WithdrawalRecord, WithdrawalRequest,
    WithdrawalStatus,
};

#[derive(Clone, Copy, Debug, Default)]
struct AssetTotals {
    finalized_deposits: Balance,
    minted: Balance,
    burned: Balance,
    completed_releases: Balance,
}

#[derive(Clone, Debug)]
pub struct ExternalAssetCoordinator {
    registry_authority: AccountId,
    assets: BTreeMap<AssetId, ExternalAssetConfig>,
    origins: BTreeMap<(OriginNetworkId, OriginAssetId), AssetId>,
    deposits: BTreeMap<DepositId, DepositRecord>,
    mint_references: BTreeMap<SettlementReference, DepositId>,
    withdrawals: BTreeMap<WithdrawalId, WithdrawalRecord>,
    burn_references: BTreeMap<SettlementReference, WithdrawalId>,
    release_transactions: BTreeMap<(OriginNetworkId, OriginTransactionId, u32), WithdrawalId>,
    totals: BTreeMap<AssetId, AssetTotals>,
    events: Vec<ExternalAssetEvent>,
}

impl ExternalAssetCoordinator {
    #[must_use]
    pub fn new(registry_authority: AccountId) -> Self {
        Self {
            registry_authority,
            assets: BTreeMap::new(),
            origins: BTreeMap::new(),
            deposits: BTreeMap::new(),
            mint_references: BTreeMap::new(),
            withdrawals: BTreeMap::new(),
            burn_references: BTreeMap::new(),
            release_transactions: BTreeMap::new(),
            totals: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    /// Registers one unambiguous origin asset mapping.
    ///
    /// # Errors
    ///
    /// Returns an error for an unauthorized caller or a duplicate platform or
    /// origin asset.
    pub fn register_asset(
        &mut self,
        origin: AccountId,
        config: ExternalAssetConfig,
    ) -> Result<(), ExternalAssetError> {
        self.require_registry(origin)?;
        if !config.has_separated_authorities(self.registry_authority) {
            return Err(ExternalAssetError::InvalidAuthorityConfiguration);
        }
        if self.assets.contains_key(&config.asset) {
            return Err(ExternalAssetError::AssetAlreadyRegistered);
        }
        let origin_key = (config.origin_network, config.origin_asset);
        if self.origins.contains_key(&origin_key) {
            return Err(ExternalAssetError::OriginAssetAlreadyRegistered);
        }
        self.origins.insert(origin_key, config.asset);
        self.totals.insert(config.asset, AssetTotals::default());
        self.assets.insert(config.asset, config);
        self.events.push(ExternalAssetEvent::AssetRegistered {
            asset: config.asset,
        });
        Ok(())
    }

    /// Changes connector admission policy without deleting in-flight records.
    ///
    /// # Errors
    ///
    /// Returns an error for an unauthorized caller or unknown asset.
    pub fn set_status(
        &mut self,
        origin: AccountId,
        asset: AssetId,
        status: ConnectorStatus,
    ) -> Result<(), ExternalAssetError> {
        self.require_registry(origin)?;
        self.config_mut(asset)?.status = status;
        self.events
            .push(ExternalAssetEvent::StatusChanged { asset, status });
        Ok(())
    }

    /// Records a deposit observation. Exact retries return the existing ID;
    /// contradictory interpretations of the same origin event are rejected.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, disabled deposits, invalid origin,
    /// zero amount or a conflicting observation.
    pub fn observe_deposit(
        &mut self,
        origin: AccountId,
        claim: OriginDeposit,
    ) -> Result<DepositId, ExternalAssetError> {
        let config = *self.config(claim.asset)?;
        require_authority(origin, config.observer_authority)?;
        if !config.status.accepts_deposits() {
            return Err(ExternalAssetError::DepositsPaused);
        }
        if claim.amount == 0 {
            return Err(ExternalAssetError::InvalidAmount);
        }
        if claim.origin_network != config.origin_network
            || claim.origin_asset != config.origin_asset
        {
            return Err(ExternalAssetError::InvalidDepositOrigin);
        }
        let id = DepositId::derive(&claim);
        if let Some(existing) = self.deposits.get(&id) {
            return if existing.claim == claim {
                Ok(id)
            } else {
                Err(ExternalAssetError::DepositReferenceConflict)
            };
        }
        self.deposits.insert(
            id,
            DepositRecord {
                id,
                claim,
                status: DepositStatus::Observed,
                finality: None,
                mint_reference: None,
            },
        );
        self.events.push(ExternalAssetEvent::DepositObserved {
            asset: claim.asset,
            deposit: id,
        });
        Ok(id)
    }

    /// Attests chain-specific finality after verification outside this core.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, unknown deposit, conflicting
    /// evidence, invalid transition or arithmetic overflow.
    pub fn finalize_deposit(
        &mut self,
        origin: AccountId,
        deposit: DepositId,
        evidence: FinalityEvidence,
    ) -> Result<(), ExternalAssetError> {
        validate_finality(evidence)?;
        let record = *self.deposit(deposit)?;
        let config = *self.config(record.claim.asset)?;
        require_authority(origin, config.finality_authority)?;
        if record.status != DepositStatus::Observed {
            return same_finality_or_transition(record.finality, evidence);
        }
        let total = self.totals_mut(record.claim.asset)?;
        total.finalized_deposits = checked_add(total.finalized_deposits, record.claim.amount)?;
        let stored = self.deposit_mut(deposit)?;
        stored.status = DepositStatus::Finalized;
        stored.finality = Some(evidence);
        self.events.push(ExternalAssetEvent::DepositFinalized {
            asset: record.claim.asset,
            deposit,
        });
        Ok(())
    }

    /// Produces the deterministic mint instruction for one finalized deposit.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, paused deposits or an invalid
    /// transition.
    pub fn authorize_mint(
        &mut self,
        origin: AccountId,
        deposit: DepositId,
    ) -> Result<MintInstruction, ExternalAssetError> {
        let record = *self.deposit(deposit)?;
        let config = *self.config(record.claim.asset)?;
        require_authority(origin, config.settlement_authority)?;
        if !config.status.accepts_deposits() {
            return Err(ExternalAssetError::DepositsPaused);
        }
        match record.status {
            DepositStatus::Finalized => {
                self.deposit_mut(deposit)?.status = DepositStatus::MintAuthorized;
                self.events.push(ExternalAssetEvent::MintAuthorized {
                    asset: record.claim.asset,
                    deposit,
                });
            }
            DepositStatus::MintAuthorized => {}
            DepositStatus::Observed | DepositStatus::Minted => {
                return Err(ExternalAssetError::InvalidTransition);
            }
        }
        Ok(record.into())
    }

    /// Records the unique ledger settlement reference after mint execution.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, invalid transition, a conflicting
    /// retry or arithmetic overflow.
    pub fn record_mint(
        &mut self,
        origin: AccountId,
        deposit: DepositId,
        reference: SettlementReference,
    ) -> Result<(), ExternalAssetError> {
        if reference.is_zero() {
            return Err(ExternalAssetError::InvalidSettlementReference);
        }
        let record = *self.deposit(deposit)?;
        let config = *self.config(record.claim.asset)?;
        require_authority(origin, config.settlement_authority)?;
        if record.status == DepositStatus::Minted {
            return if record.mint_reference == Some(reference) {
                Ok(())
            } else {
                Err(ExternalAssetError::SettlementReferenceConflict)
            };
        }
        if record.status != DepositStatus::MintAuthorized {
            return Err(ExternalAssetError::InvalidTransition);
        }
        if self.mint_references.contains_key(&reference) {
            return Err(ExternalAssetError::SettlementReferenceConflict);
        }
        let totals = self.totals_mut(record.claim.asset)?;
        let minted = checked_add(totals.minted, record.claim.amount)?;
        if minted > totals.finalized_deposits {
            return Err(ExternalAssetError::ArithmeticOverflow);
        }
        totals.minted = minted;
        self.mint_references.insert(reference, deposit);
        let stored = self.deposit_mut(deposit)?;
        stored.status = DepositStatus::Minted;
        stored.mint_reference = Some(reference);
        self.events.push(ExternalAssetEvent::MintRecorded {
            asset: record.claim.asset,
            deposit,
        });
        Ok(())
    }

    /// Registers a withdrawal only after an authorized component attests a
    /// unique ledger burn reference.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, paused withdrawals, zero amount,
    /// a conflicting burn reference or withdrawal above minted supply.
    pub fn record_burn(
        &mut self,
        origin: AccountId,
        request: WithdrawalRequest,
    ) -> Result<WithdrawalId, ExternalAssetError> {
        if request.burn_reference.is_zero() {
            return Err(ExternalAssetError::InvalidSettlementReference);
        }
        if request.destination.is_zero() {
            return Err(ExternalAssetError::InvalidDestination);
        }
        let config = *self.config(request.asset)?;
        require_authority(origin, config.settlement_authority)?;
        if !config.status.accepts_withdrawals() {
            return Err(ExternalAssetError::WithdrawalsPaused);
        }
        if request.amount == 0 {
            return Err(ExternalAssetError::InvalidAmount);
        }
        let id = WithdrawalId::derive(&request);
        if let Some(existing_id) = self.burn_references.get(&request.burn_reference) {
            let existing = self.withdrawal(*existing_id)?;
            return if existing.id == id && existing.request == request {
                Ok(id)
            } else {
                Err(ExternalAssetError::BurnReferenceConflict)
            };
        }
        let totals = self.totals_mut(request.asset)?;
        let burned = checked_add(totals.burned, request.amount)?;
        if burned > totals.minted {
            return Err(ExternalAssetError::InsufficientMintedSupply);
        }
        totals.burned = burned;
        self.burn_references.insert(request.burn_reference, id);
        self.withdrawals.insert(
            id,
            WithdrawalRecord {
                id,
                request,
                status: WithdrawalStatus::BurnVerified,
                origin_release: None,
                finality: None,
            },
        );
        self.events.push(ExternalAssetEvent::WithdrawalRecorded {
            asset: request.asset,
            withdrawal: id,
        });
        Ok(id)
    }

    /// Produces a release instruction after burn verification.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, paused withdrawals or an invalid
    /// transition.
    pub fn authorize_release(
        &mut self,
        origin: AccountId,
        withdrawal: WithdrawalId,
    ) -> Result<ReleaseInstruction, ExternalAssetError> {
        let record = *self.withdrawal(withdrawal)?;
        let config = *self.config(record.request.asset)?;
        require_authority(origin, config.release_authority)?;
        if !config.status.accepts_withdrawals() {
            return Err(ExternalAssetError::WithdrawalsPaused);
        }
        match record.status {
            WithdrawalStatus::BurnVerified => {
                self.withdrawal_mut(withdrawal)?.status = WithdrawalStatus::ReleaseAuthorized;
                self.events.push(ExternalAssetEvent::ReleaseAuthorized {
                    asset: record.request.asset,
                    withdrawal,
                });
            }
            WithdrawalStatus::ReleaseAuthorized => {}
            WithdrawalStatus::Submitted | WithdrawalStatus::Completed => {
                return Err(ExternalAssetError::InvalidTransition);
            }
        }
        Ok(ReleaseInstruction {
            withdrawal,
            asset: record.request.asset,
            origin_network: config.origin_network,
            origin_asset: config.origin_asset,
            amount: record.request.amount,
            destination: record.request.destination,
        })
    }

    /// Records the unique origin-chain transaction event/output used for release.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, invalid transition or reuse of an
    /// origin event/output for another withdrawal.
    pub fn record_release_submission(
        &mut self,
        origin: AccountId,
        withdrawal: WithdrawalId,
        release: OriginReleaseReference,
    ) -> Result<(), ExternalAssetError> {
        let record = *self.withdrawal(withdrawal)?;
        let config = *self.config(record.request.asset)?;
        require_authority(origin, config.release_authority)?;
        if matches!(
            record.status,
            WithdrawalStatus::Submitted | WithdrawalStatus::Completed
        ) {
            return if record.origin_release == Some(release) {
                Ok(())
            } else {
                Err(ExternalAssetError::OriginTransactionConflict)
            };
        }
        if record.status != WithdrawalStatus::ReleaseAuthorized {
            return Err(ExternalAssetError::InvalidTransition);
        }
        let key = (
            config.origin_network,
            release.transaction,
            release.event_index,
        );
        if self.release_transactions.contains_key(&key) {
            return Err(ExternalAssetError::OriginTransactionConflict);
        }
        self.release_transactions.insert(key, withdrawal);
        let stored = self.withdrawal_mut(withdrawal)?;
        stored.status = WithdrawalStatus::Submitted;
        stored.origin_release = Some(release);
        self.events.push(ExternalAssetEvent::ReleaseSubmitted {
            asset: record.request.asset,
            withdrawal,
        });
        Ok(())
    }

    /// Marks a submitted release complete after origin-chain finality.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong authority, invalid transition, conflicting
    /// evidence or arithmetic overflow.
    pub fn complete_withdrawal(
        &mut self,
        origin: AccountId,
        withdrawal: WithdrawalId,
        evidence: FinalityEvidence,
    ) -> Result<(), ExternalAssetError> {
        validate_finality(evidence)?;
        let record = *self.withdrawal(withdrawal)?;
        let config = *self.config(record.request.asset)?;
        require_authority(origin, config.finality_authority)?;
        if record.status == WithdrawalStatus::Completed {
            return same_finality_or_transition(record.finality, evidence);
        }
        if record.status != WithdrawalStatus::Submitted {
            return Err(ExternalAssetError::InvalidTransition);
        }
        let totals = self.totals_mut(record.request.asset)?;
        let completed = checked_add(totals.completed_releases, record.request.amount)?;
        if completed > totals.burned {
            return Err(ExternalAssetError::ArithmeticOverflow);
        }
        totals.completed_releases = completed;
        let stored = self.withdrawal_mut(withdrawal)?;
        stored.status = WithdrawalStatus::Completed;
        stored.finality = Some(evidence);
        self.events.push(ExternalAssetEvent::WithdrawalCompleted {
            asset: record.request.asset,
            withdrawal,
        });
        Ok(())
    }

    #[must_use]
    pub fn asset_config(&self, asset: AssetId) -> Option<ExternalAssetConfig> {
        self.assets.get(&asset).copied()
    }

    #[must_use]
    pub fn deposit_record(&self, deposit: DepositId) -> Option<DepositRecord> {
        self.deposits.get(&deposit).copied()
    }

    #[must_use]
    pub fn withdrawal_record(&self, withdrawal: WithdrawalId) -> Option<WithdrawalRecord> {
        self.withdrawals.get(&withdrawal).copied()
    }

    /// Reconciles finalized custody inflows/outflows with representation
    /// mint/burn totals.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown asset or impossible arithmetic.
    pub fn audit(&self, asset: AssetId) -> Result<AssetSettlementAudit, ExternalAssetError> {
        self.config(asset)?;
        let totals = self
            .totals
            .get(&asset)
            .ok_or(ExternalAssetError::AssetNotFound)?;
        let expected_backing = totals
            .finalized_deposits
            .checked_sub(totals.completed_releases)
            .ok_or(ExternalAssetError::ArithmeticOverflow)?;
        let represented_supply = totals
            .minted
            .checked_sub(totals.burned)
            .ok_or(ExternalAssetError::ArithmeticOverflow)?;
        let backing_surplus = expected_backing
            .checked_sub(represented_supply)
            .ok_or(ExternalAssetError::ArithmeticOverflow)?;
        Ok(AssetSettlementAudit {
            finalized_deposits: totals.finalized_deposits,
            minted: totals.minted,
            burned: totals.burned,
            completed_releases: totals.completed_releases,
            expected_backing,
            represented_supply,
            backing_surplus,
        })
    }

    #[must_use]
    pub fn events(&self) -> &[ExternalAssetEvent] {
        &self.events
    }

    fn require_registry(&self, origin: AccountId) -> Result<(), ExternalAssetError> {
        require_authority(origin, self.registry_authority)
    }

    fn config(&self, asset: AssetId) -> Result<&ExternalAssetConfig, ExternalAssetError> {
        self.assets
            .get(&asset)
            .ok_or(ExternalAssetError::AssetNotFound)
    }

    fn config_mut(
        &mut self,
        asset: AssetId,
    ) -> Result<&mut ExternalAssetConfig, ExternalAssetError> {
        self.assets
            .get_mut(&asset)
            .ok_or(ExternalAssetError::AssetNotFound)
    }

    fn deposit(&self, id: DepositId) -> Result<&DepositRecord, ExternalAssetError> {
        self.deposits
            .get(&id)
            .ok_or(ExternalAssetError::DepositNotFound)
    }

    fn deposit_mut(&mut self, id: DepositId) -> Result<&mut DepositRecord, ExternalAssetError> {
        self.deposits
            .get_mut(&id)
            .ok_or(ExternalAssetError::DepositNotFound)
    }

    fn withdrawal(&self, id: WithdrawalId) -> Result<&WithdrawalRecord, ExternalAssetError> {
        self.withdrawals
            .get(&id)
            .ok_or(ExternalAssetError::WithdrawalNotFound)
    }

    fn withdrawal_mut(
        &mut self,
        id: WithdrawalId,
    ) -> Result<&mut WithdrawalRecord, ExternalAssetError> {
        self.withdrawals
            .get_mut(&id)
            .ok_or(ExternalAssetError::WithdrawalNotFound)
    }

    fn totals_mut(&mut self, asset: AssetId) -> Result<&mut AssetTotals, ExternalAssetError> {
        self.totals
            .get_mut(&asset)
            .ok_or(ExternalAssetError::AssetNotFound)
    }
}

fn require_authority(origin: AccountId, expected: AccountId) -> Result<(), ExternalAssetError> {
    if origin == expected {
        Ok(())
    } else {
        Err(ExternalAssetError::Unauthorized)
    }
}

fn checked_add(left: Balance, right: Balance) -> Result<Balance, ExternalAssetError> {
    left.checked_add(right)
        .ok_or(ExternalAssetError::ArithmeticOverflow)
}

fn same_finality_or_transition(
    existing: Option<FinalityEvidence>,
    supplied: FinalityEvidence,
) -> Result<(), ExternalAssetError> {
    match existing {
        Some(value) if value == supplied => Ok(()),
        Some(_) => Err(ExternalAssetError::FinalityConflict),
        None => Err(ExternalAssetError::InvalidTransition),
    }
}

fn validate_finality(evidence: FinalityEvidence) -> Result<(), ExternalAssetError> {
    if evidence.block_hash.iter().all(|byte| *byte == 0)
        || evidence.proof_hash.iter().all(|byte| *byte == 0)
    {
        Err(ExternalAssetError::InvalidFinalityEvidence)
    } else {
        Ok(())
    }
}
