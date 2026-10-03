use crate::{
    AccountId, AssetDefinition, AssetId, BackingRequirement, Balance, Ledger, LedgerError,
    NetworkId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenesisAsset {
    pub definition: AssetDefinition,
    pub verified_backing: Balance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenesisBalance {
    pub asset: AssetId,
    pub account: AccountId,
    pub amount: Balance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenesisConfig {
    pub network: NetworkId,
    pub registry_authority: AccountId,
    pub assets: Vec<GenesisAsset>,
    pub balances: Vec<GenesisBalance>,
}

pub(crate) fn build(config: GenesisConfig) -> Result<Ledger, LedgerError> {
    validate_canonical_order(&config)?;

    let registry_authority = config.registry_authority;
    let mut ledger = Ledger::new(config.network, registry_authority);

    for asset in config.assets {
        let id = asset.definition.id;
        let backing_authority = asset.definition.backing_authority;
        let backing_requirement = asset.definition.backing_requirement;
        if backing_requirement == BackingRequirement::None && asset.verified_backing != 0 {
            return Err(LedgerError::UnexpectedGenesisBacking);
        }

        ledger.register_asset(registry_authority, asset.definition)?;
        if backing_requirement == BackingRequirement::VerifiedOneToOne {
            ledger.observe_backing(backing_authority, id, asset.verified_backing)?;
        }
    }

    for allocation in config.balances {
        let issuer = ledger
            .definition(allocation.asset)
            .ok_or(LedgerError::AssetNotFound)?
            .issuer;
        ledger.mint(
            issuer,
            allocation.asset,
            allocation.account,
            allocation.amount,
        )?;
    }

    Ok(ledger)
}

fn validate_canonical_order(config: &GenesisConfig) -> Result<(), LedgerError> {
    let assets_are_canonical = config
        .assets
        .windows(2)
        .all(|pair| pair[0].definition.id < pair[1].definition.id);
    if !assets_are_canonical {
        return Err(LedgerError::NonCanonicalGenesisAssets);
    }

    let balances_are_canonical = config
        .balances
        .windows(2)
        .all(|pair| (pair[0].asset, pair[0].account) < (pair[1].asset, pair[1].account));
    if !balances_are_canonical {
        return Err(LedgerError::NonCanonicalGenesisBalances);
    }
    Ok(())
}
