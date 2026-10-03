use crate::{AccountId, AssetId, Balance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetClass {
    NetworkNative,
    Sovereign,
    IssuerRegulated,
    ExternalRepresentation,
    Application,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetStatus {
    Proposed,
    Test,
    Active,
    WithdrawOnly,
    Suspended,
    Retired,
}

impl AssetStatus {
    pub(crate) const fn allows_transfer(self) -> bool {
        matches!(self, Self::Test | Self::Active)
    }

    pub(crate) const fn allows_mint(self) -> bool {
        matches!(self, Self::Test | Self::Active)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackingRequirement {
    None,
    VerifiedOneToOne,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetDefinition {
    pub id: AssetId,
    pub symbol: String,
    pub decimals: u8,
    pub class: AssetClass,
    pub status: AssetStatus,
    pub issuer: AccountId,
    pub backing_authority: AccountId,
    pub freeze_authority: AccountId,
    pub treasury: AccountId,
    pub max_supply: Option<Balance>,
    pub backing_requirement: BackingRequirement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetAudit {
    pub declared_supply: Balance,
    pub summed_balances: Balance,
    pub verified_backing: Balance,
    pub supply_matches_balances: bool,
    pub backing_covers_supply: bool,
    pub status: AssetStatus,
}

#[derive(Clone, Debug)]
pub(crate) struct AssetState {
    pub(crate) definition: AssetDefinition,
    pub(crate) supply: Balance,
    pub(crate) verified_backing: Balance,
}
