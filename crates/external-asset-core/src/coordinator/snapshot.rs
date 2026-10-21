use ledger_core::Balance;

use super::{AssetTotals, ExternalAssetCoordinator, validate_finality};
use crate::{
    DepositId, DepositRecord, DepositStatus, ExternalAssetConfig, ExternalAssetError, WithdrawalId,
    WithdrawalRecord, WithdrawalStatus,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalAssetSnapshot {
    pub registry_authority: ledger_core::AccountId,
    pub assets: Vec<ExternalAssetConfig>,
    pub deposits: Vec<DepositRecord>,
    pub withdrawals: Vec<WithdrawalRecord>,
}

impl ExternalAssetCoordinator {
    /// Exports canonical bridge settlement state. Ephemeral event history is
    /// deliberately excluded from the current-state snapshot.
    #[must_use]
    pub fn snapshot(&self) -> ExternalAssetSnapshot {
        ExternalAssetSnapshot {
            registry_authority: self.registry_authority,
            assets: self.assets.values().copied().collect(),
            deposits: self.deposits.values().copied().collect(),
            withdrawals: self.withdrawals.values().copied().collect(),
        }
    }

    /// Restores canonical bridge state after rebuilding and validating every
    /// replay index and monetary total.
    ///
    /// # Errors
    ///
    /// Returns an error for non-canonical ordering, invalid records, duplicate
    /// references, impossible lifecycle state or broken monetary invariants.
    pub fn from_snapshot(snapshot: ExternalAssetSnapshot) -> Result<Self, ExternalAssetError> {
        require_canonical(&snapshot)?;
        let mut restored = Self::new(snapshot.registry_authority);
        restore_assets(&mut restored, snapshot.assets)?;
        restore_deposits(&mut restored, snapshot.deposits)?;
        restore_withdrawals(&mut restored, snapshot.withdrawals)?;
        validate_totals(&restored)?;
        restored.events.clear();
        Ok(restored)
    }
}

fn require_canonical(snapshot: &ExternalAssetSnapshot) -> Result<(), ExternalAssetError> {
    let assets = snapshot
        .assets
        .windows(2)
        .all(|pair| pair[0].asset < pair[1].asset);
    let deposits = snapshot
        .deposits
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id);
    let withdrawals = snapshot
        .withdrawals
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id);
    if assets && deposits && withdrawals {
        Ok(())
    } else {
        Err(ExternalAssetError::NonCanonicalSnapshot)
    }
}

fn restore_assets(
    restored: &mut ExternalAssetCoordinator,
    assets: Vec<ExternalAssetConfig>,
) -> Result<(), ExternalAssetError> {
    for config in assets {
        if !config.has_separated_authorities(restored.registry_authority) {
            return Err(ExternalAssetError::InvalidAuthorityConfiguration);
        }
        let origin = (config.origin_network, config.origin_asset);
        if restored.origins.insert(origin, config.asset).is_some()
            || restored.assets.insert(config.asset, config).is_some()
        {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
        restored.totals.insert(config.asset, AssetTotals::default());
    }
    Ok(())
}

fn restore_deposits(
    restored: &mut ExternalAssetCoordinator,
    deposits: Vec<DepositRecord>,
) -> Result<(), ExternalAssetError> {
    for record in deposits {
        if record.id != DepositId::derive(&record.claim) || record.claim.amount == 0 {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
        let config = restored
            .assets
            .get(&record.claim.asset)
            .ok_or(ExternalAssetError::SnapshotInvariant)?;
        if record.claim.origin_network != config.origin_network
            || record.claim.origin_asset != config.origin_asset
        {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
        validate_deposit_shape(&record)?;
        let totals = restored
            .totals
            .get_mut(&record.claim.asset)
            .ok_or(ExternalAssetError::SnapshotInvariant)?;
        if record.status != DepositStatus::Observed {
            totals.finalized_deposits = add(totals.finalized_deposits, record.claim.amount)?;
        }
        if record.status == DepositStatus::Minted {
            totals.minted = add(totals.minted, record.claim.amount)?;
            let reference = record
                .mint_reference
                .ok_or(ExternalAssetError::SnapshotInvariant)?;
            if restored
                .mint_references
                .insert(reference, record.id)
                .is_some()
            {
                return Err(ExternalAssetError::SnapshotInvariant);
            }
        }
        restored.deposits.insert(record.id, record);
    }
    Ok(())
}

fn validate_deposit_shape(record: &DepositRecord) -> Result<(), ExternalAssetError> {
    match record.status {
        DepositStatus::Observed if record.finality.is_none() && record.mint_reference.is_none() => {
            Ok(())
        }
        DepositStatus::Finalized | DepositStatus::MintAuthorized
            if record.finality.is_some() && record.mint_reference.is_none() =>
        {
            validate_finality(
                record
                    .finality
                    .ok_or(ExternalAssetError::SnapshotInvariant)?,
            )
        }
        DepositStatus::Minted if record.finality.is_some() && record.mint_reference.is_some() => {
            validate_finality(
                record
                    .finality
                    .ok_or(ExternalAssetError::SnapshotInvariant)?,
            )?;
            if record
                .mint_reference
                .is_some_and(crate::SettlementReference::is_zero)
            {
                return Err(ExternalAssetError::SnapshotInvariant);
            }
            Ok(())
        }
        _ => Err(ExternalAssetError::SnapshotInvariant),
    }
}

fn restore_withdrawals(
    restored: &mut ExternalAssetCoordinator,
    withdrawals: Vec<WithdrawalRecord>,
) -> Result<(), ExternalAssetError> {
    for record in withdrawals {
        if record.id != WithdrawalId::derive(&record.request)
            || record.request.amount == 0
            || record.request.burn_reference.is_zero()
            || record.request.destination.is_zero()
        {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
        let config = restored
            .assets
            .get(&record.request.asset)
            .ok_or(ExternalAssetError::SnapshotInvariant)?;
        validate_withdrawal_shape(&record)?;
        if restored
            .burn_references
            .insert(record.request.burn_reference, record.id)
            .is_some()
        {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
        if let Some(reference) = record.origin_release {
            let key = (
                config.origin_network,
                reference.transaction,
                reference.event_index,
            );
            if restored
                .release_transactions
                .insert(key, record.id)
                .is_some()
            {
                return Err(ExternalAssetError::SnapshotInvariant);
            }
        }
        let totals = restored
            .totals
            .get_mut(&record.request.asset)
            .ok_or(ExternalAssetError::SnapshotInvariant)?;
        totals.burned = add(totals.burned, record.request.amount)?;
        if record.status == WithdrawalStatus::Completed {
            totals.completed_releases = add(totals.completed_releases, record.request.amount)?;
        }
        restored.withdrawals.insert(record.id, record);
    }
    Ok(())
}

fn validate_withdrawal_shape(record: &WithdrawalRecord) -> Result<(), ExternalAssetError> {
    match record.status {
        WithdrawalStatus::BurnVerified | WithdrawalStatus::ReleaseAuthorized
            if record.origin_release.is_none() && record.finality.is_none() =>
        {
            Ok(())
        }
        WithdrawalStatus::Submitted
            if record.origin_release.is_some() && record.finality.is_none() =>
        {
            Ok(())
        }
        WithdrawalStatus::Completed
            if record.origin_release.is_some() && record.finality.is_some() =>
        {
            validate_finality(
                record
                    .finality
                    .ok_or(ExternalAssetError::SnapshotInvariant)?,
            )
        }
        _ => Err(ExternalAssetError::SnapshotInvariant),
    }
}

fn validate_totals(restored: &ExternalAssetCoordinator) -> Result<(), ExternalAssetError> {
    for asset in restored.assets.keys() {
        let totals = restored
            .totals
            .get(asset)
            .ok_or(ExternalAssetError::SnapshotInvariant)?;
        if totals.minted > totals.finalized_deposits
            || totals.burned > totals.minted
            || totals.completed_releases > totals.burned
        {
            return Err(ExternalAssetError::SnapshotInvariant);
        }
    }
    Ok(())
}

fn add(left: Balance, right: Balance) -> Result<Balance, ExternalAssetError> {
    left.checked_add(right)
        .ok_or(ExternalAssetError::ArithmeticOverflow)
}
