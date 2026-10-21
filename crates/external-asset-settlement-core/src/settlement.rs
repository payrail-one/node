use external_asset_core::{
    DepositId, ExternalAssetConfig, ExternalAssetCoordinator, ExternalAssetError,
    ExternalAssetSnapshot, FinalityEvidence, OriginDeposit, OriginReleaseReference,
    ReleaseInstruction, SettlementReference, WithdrawalId,
};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, BackingRequirement, Ledger, LedgerSnapshot,
    NetworkId,
};

use crate::{AuthorizedWithdrawal, ExternalSettlementError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementOutcome<Identifier> {
    Applied(Identifier),
    ExistingSame(Identifier),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAssetSettlementSnapshot {
    pub ledger: LedgerSnapshot,
    pub external: ExternalAssetSnapshot,
}

#[derive(Clone, Debug)]
pub struct ExternalAssetSettlement {
    ledger: Ledger,
    external: ExternalAssetCoordinator,
}

impl ExternalAssetSettlement {
    #[must_use]
    pub const fn new(ledger: Ledger, external: ExternalAssetCoordinator) -> Self {
        Self { ledger, external }
    }

    #[must_use]
    pub const fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    #[must_use]
    pub const fn external(&self) -> &ExternalAssetCoordinator {
        &self.external
    }

    #[must_use]
    pub fn into_parts(self) -> (Ledger, ExternalAssetCoordinator) {
        (self.ledger, self.external)
    }

    #[must_use]
    pub fn snapshot(&self) -> ExternalAssetSettlementSnapshot {
        ExternalAssetSettlementSnapshot {
            ledger: self.ledger.snapshot(),
            external: self.external.snapshot(),
        }
    }

    /// Restores and cross-reconciles complete monetary and connector state.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid component snapshot, unmatched external
    /// representation, or supply/backing disagreement.
    pub fn from_snapshot(
        network: NetworkId,
        snapshot: ExternalAssetSettlementSnapshot,
    ) -> Result<Self, ExternalSettlementError> {
        let ledger = Ledger::from_snapshot(network, snapshot.ledger)?;
        let external = ExternalAssetCoordinator::from_snapshot(snapshot.external)?;
        let restored = Self { ledger, external };
        for asset in restored.ledger.snapshot().assets {
            if asset.definition.class == AssetClass::ExternalRepresentation
                && restored
                    .external
                    .asset_config(asset.definition.id)
                    .is_none()
            {
                return Err(ExternalSettlementError::AssetDefinitionMismatch);
            }
        }
        for config in restored.external.snapshot().assets {
            restored.validate_reconciliation(config.asset)?;
        }
        Ok(restored)
    }

    /// Atomically registers matching ledger and external-settlement assets.
    ///
    /// # Errors
    ///
    /// Returns an error when authorities, asset identities or one-to-one
    /// backing policy disagree. Neither state machine changes on failure.
    pub fn register_asset(
        &mut self,
        registry: AccountId,
        definition: AssetDefinition,
        config: ExternalAssetConfig,
    ) -> Result<(), ExternalSettlementError> {
        validate_definition(&definition, config)?;
        let mut candidate = self.clone();
        candidate.ledger.register_asset(registry, definition)?;
        candidate.external.register_asset(registry, config)?;
        candidate.validate_reconciliation(config.asset)?;
        *self = candidate;
        Ok(())
    }

    /// Atomically records one finalized origin deposit, updates verified
    /// backing, mints its exact amount and records the settlement reference.
    /// Exact retries do not mint twice.
    ///
    /// # Errors
    ///
    /// Returns an error for authority, evidence, policy, ledger, reference or
    /// reconciliation failures. Neither state machine changes on failure.
    pub fn settle_deposit(
        &mut self,
        observer: AccountId,
        finality: AccountId,
        settlement: AccountId,
        claim: OriginDeposit,
        evidence: FinalityEvidence,
        reference: SettlementReference,
    ) -> Result<SettlementOutcome<DepositId>, ExternalSettlementError> {
        let config = self.require_roles(claim.asset, observer, finality, settlement)?;
        self.validate_reconciliation(claim.asset)?;
        let id = DepositId::derive(&claim);
        if let Some(record) = self.external.deposit_record(id) {
            if record.claim != claim {
                return Err(ExternalAssetError::DepositReferenceConflict.into());
            }
            if record.status == external_asset_core::DepositStatus::Minted {
                if record.finality != Some(evidence) {
                    return Err(ExternalAssetError::FinalityConflict.into());
                }
                if record.mint_reference != Some(reference) {
                    return Err(ExternalAssetError::SettlementReferenceConflict.into());
                }
                return Ok(SettlementOutcome::ExistingSame(id));
            }
        }

        let mut candidate = self.clone();
        let deposit_id = candidate.external.observe_deposit(observer, claim)?;
        candidate
            .external
            .finalize_deposit(finality, deposit_id, evidence)?;
        let audit = candidate.external.audit(claim.asset)?;
        candidate
            .ledger
            .observe_backing(finality, claim.asset, audit.expected_backing)?;
        let instruction = candidate.external.authorize_mint(settlement, deposit_id)?;
        if instruction.asset != claim.asset
            || instruction.recipient != claim.recipient
            || instruction.amount != claim.amount
        {
            return Err(ExternalSettlementError::InstructionMismatch);
        }
        candidate.ledger.mint(
            config.settlement_authority,
            instruction.asset,
            instruction.recipient,
            instruction.amount,
        )?;
        candidate
            .external
            .record_mint(settlement, deposit_id, reference)?;
        candidate.validate_reconciliation(claim.asset)?;
        *self = candidate;
        Ok(SettlementOutcome::Applied(id))
    }

    /// Atomically burns represented supply after owner authorization and
    /// records the unique withdrawal. Exact retries do not burn twice.
    ///
    /// # Errors
    ///
    /// Returns an error for authority, balance, reference, policy or
    /// reconciliation failures. Neither state machine changes on failure.
    pub fn burn_withdrawal(
        &mut self,
        settlement: AccountId,
        current_height: u64,
        authorized: AuthorizedWithdrawal,
    ) -> Result<SettlementOutcome<WithdrawalId>, ExternalSettlementError> {
        if authorized.network != self.ledger.network() {
            return Err(ExternalSettlementError::WrongWithdrawalNetwork);
        }
        if current_height > authorized.valid_until_height {
            return Err(ExternalSettlementError::ExpiredWithdrawalAuthorization);
        }
        let request = authorized.request;
        let config = self
            .external
            .asset_config(request.asset)
            .ok_or(ExternalAssetError::AssetNotFound)?;
        if config.settlement_authority != settlement {
            return Err(ExternalAssetError::Unauthorized.into());
        }
        self.validate_reconciliation(request.asset)?;
        let id = WithdrawalId::derive(&request);
        if let Some(record) = self.external.withdrawal_record(id) {
            return if record.request == request {
                Ok(SettlementOutcome::ExistingSame(id))
            } else {
                Err(ExternalAssetError::BurnReferenceConflict.into())
            };
        }

        let mut candidate = self.clone();
        candidate.ledger.burn(
            config.settlement_authority,
            request.asset,
            request.account,
            request.amount,
        )?;
        let withdrawal = candidate.external.record_burn(settlement, request)?;
        candidate.validate_reconciliation(request.asset)?;
        *self = candidate;
        Ok(SettlementOutcome::Applied(withdrawal))
    }

    /// Atomically advances a verified burn to a deterministic custody release
    /// instruction.
    ///
    /// # Errors
    ///
    /// Returns the external state-machine error without partial mutation.
    pub fn authorize_release(
        &mut self,
        release_authority: AccountId,
        withdrawal: WithdrawalId,
    ) -> Result<ReleaseInstruction, ExternalSettlementError> {
        let mut candidate = self.clone();
        let instruction = candidate
            .external
            .authorize_release(release_authority, withdrawal)?;
        *self = candidate;
        Ok(instruction)
    }

    /// Atomically records the Ethereum release transaction reference.
    ///
    /// # Errors
    ///
    /// Returns the external state-machine error without partial mutation.
    pub fn record_release_submission(
        &mut self,
        release_authority: AccountId,
        withdrawal: WithdrawalId,
        reference: OriginReleaseReference,
    ) -> Result<(), ExternalSettlementError> {
        let mut candidate = self.clone();
        candidate
            .external
            .record_release_submission(release_authority, withdrawal, reference)?;
        *self = candidate;
        Ok(())
    }

    /// Atomically finalizes an Ethereum release and decreases the ledger's
    /// verified backing to the reconciled custody amount.
    ///
    /// # Errors
    ///
    /// Returns an error for authority, evidence, ledger or reconciliation
    /// failures. Neither state machine changes on failure.
    pub fn complete_withdrawal(
        &mut self,
        finality: AccountId,
        withdrawal: WithdrawalId,
        evidence: FinalityEvidence,
    ) -> Result<(), ExternalSettlementError> {
        let record = self
            .external
            .withdrawal_record(withdrawal)
            .ok_or(ExternalAssetError::WithdrawalNotFound)?;
        let config = self
            .external
            .asset_config(record.request.asset)
            .ok_or(ExternalAssetError::AssetNotFound)?;
        if config.finality_authority != finality {
            return Err(ExternalAssetError::Unauthorized.into());
        }
        self.validate_reconciliation(record.request.asset)?;
        let mut candidate = self.clone();
        candidate
            .external
            .complete_withdrawal(finality, withdrawal, evidence)?;
        let audit = candidate.external.audit(record.request.asset)?;
        candidate
            .ledger
            .observe_backing(finality, record.request.asset, audit.expected_backing)?;
        candidate.validate_reconciliation(record.request.asset)?;
        *self = candidate;
        Ok(())
    }

    fn require_roles(
        &self,
        asset: AssetId,
        observer: AccountId,
        finality: AccountId,
        settlement: AccountId,
    ) -> Result<ExternalAssetConfig, ExternalSettlementError> {
        let config = self
            .external
            .asset_config(asset)
            .ok_or(ExternalAssetError::AssetNotFound)?;
        if config.observer_authority != observer
            || config.finality_authority != finality
            || config.settlement_authority != settlement
        {
            return Err(ExternalAssetError::Unauthorized.into());
        }
        validate_ledger_policy(self.ledger.definition(asset), config)?;
        Ok(config)
    }

    fn validate_reconciliation(&self, asset: AssetId) -> Result<(), ExternalSettlementError> {
        let config = self
            .external
            .asset_config(asset)
            .ok_or(ExternalAssetError::AssetNotFound)?;
        validate_ledger_policy(self.ledger.definition(asset), config)?;
        let ledger = self.ledger.audit_asset(asset)?;
        let external = self.external.audit(asset)?;
        if ledger.declared_supply != external.represented_supply {
            return Err(ExternalSettlementError::SupplyMismatch);
        }
        if ledger.verified_backing != external.expected_backing {
            return Err(ExternalSettlementError::BackingMismatch);
        }
        Ok(())
    }
}

fn validate_definition(
    definition: &AssetDefinition,
    config: ExternalAssetConfig,
) -> Result<(), ExternalSettlementError> {
    if definition.id != config.asset {
        return Err(ExternalSettlementError::AssetDefinitionMismatch);
    }
    validate_ledger_policy(Some(definition), config)
}

fn validate_ledger_policy(
    definition: Option<&AssetDefinition>,
    config: ExternalAssetConfig,
) -> Result<(), ExternalSettlementError> {
    let definition = definition.ok_or(ExternalSettlementError::AssetDefinitionMismatch)?;
    if definition.class != AssetClass::ExternalRepresentation
        || definition.backing_requirement != BackingRequirement::VerifiedOneToOne
        || definition.issuer != config.settlement_authority
        || definition.backing_authority != config.finality_authority
    {
        return Err(ExternalSettlementError::AssetDefinitionMismatch);
    }
    Ok(())
}
