use ledger_core::{AccountId, AssetId, Balance};
use sha2::{Digest, Sha256};

macro_rules! byte_identifier {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 32]);

        impl $name {
            #[must_use]
            pub const fn new(value: [u8; 32]) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            #[must_use]
            pub fn is_zero(self) -> bool {
                self.0.iter().all(|byte| *byte == 0)
            }
        }
    };
}

byte_identifier!(OriginNetworkId);
byte_identifier!(OriginAssetId);
byte_identifier!(OriginTransactionId);
byte_identifier!(SettlementReference);
byte_identifier!(DestinationCommitment);
byte_identifier!(DepositId);
byte_identifier!(WithdrawalId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectorStatus {
    Active,
    DepositsPaused,
    WithdrawalsPaused,
    Suspended,
    Retired,
}

impl ConnectorStatus {
    pub(crate) const fn accepts_deposits(self) -> bool {
        matches!(self, Self::Active | Self::WithdrawalsPaused)
    }

    pub(crate) const fn accepts_withdrawals(self) -> bool {
        matches!(self, Self::Active | Self::DepositsPaused)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalAssetConfig {
    pub asset: AssetId,
    pub origin_network: OriginNetworkId,
    pub origin_asset: OriginAssetId,
    pub observer_authority: AccountId,
    pub finality_authority: AccountId,
    pub settlement_authority: AccountId,
    pub release_authority: AccountId,
    pub status: ConnectorStatus,
}

impl ExternalAssetConfig {
    pub(crate) fn has_separated_authorities(self, registry: AccountId) -> bool {
        let authorities = [
            registry,
            self.observer_authority,
            self.finality_authority,
            self.settlement_authority,
            self.release_authority,
        ];
        authorities.iter().enumerate().all(|(index, authority)| {
            authority.as_bytes().iter().any(|byte| *byte != 0)
                && !authorities[..index].contains(authority)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginDeposit {
    pub asset: AssetId,
    pub origin_network: OriginNetworkId,
    pub origin_asset: OriginAssetId,
    pub transaction: OriginTransactionId,
    pub event_index: u32,
    pub recipient: AccountId,
    pub amount: Balance,
}

impl DepositId {
    #[must_use]
    pub fn derive(claim: &OriginDeposit) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"external.deposit-id\0");
        hasher.update(claim.origin_network.as_bytes());
        hasher.update(claim.origin_asset.as_bytes());
        hasher.update(claim.transaction.as_bytes());
        hasher.update(claim.event_index.to_be_bytes());
        Self(hasher.finalize().into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalityEvidence {
    pub block_height: u64,
    pub block_hash: [u8; 32],
    pub proof_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositStatus {
    Observed,
    Finalized,
    MintAuthorized,
    Minted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DepositRecord {
    pub id: DepositId,
    pub claim: OriginDeposit,
    pub status: DepositStatus,
    pub finality: Option<FinalityEvidence>,
    pub mint_reference: Option<SettlementReference>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MintInstruction {
    pub deposit: DepositId,
    pub asset: AssetId,
    pub recipient: AccountId,
    pub amount: Balance,
}

impl From<DepositRecord> for MintInstruction {
    fn from(record: DepositRecord) -> Self {
        Self {
            deposit: record.id,
            asset: record.claim.asset,
            recipient: record.claim.recipient,
            amount: record.claim.amount,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WithdrawalRequest {
    pub asset: AssetId,
    pub account: AccountId,
    pub amount: Balance,
    pub destination: DestinationCommitment,
    pub burn_reference: SettlementReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginReleaseReference {
    pub transaction: OriginTransactionId,
    pub event_index: u32,
}

impl WithdrawalId {
    #[must_use]
    pub fn derive(request: &WithdrawalRequest) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"external.withdrawal-id\0");
        hasher.update(request.asset.as_bytes());
        hasher.update(request.burn_reference.as_bytes());
        Self(hasher.finalize().into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithdrawalStatus {
    BurnVerified,
    ReleaseAuthorized,
    Submitted,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WithdrawalRecord {
    pub id: WithdrawalId,
    pub request: WithdrawalRequest,
    pub status: WithdrawalStatus,
    pub origin_release: Option<OriginReleaseReference>,
    pub finality: Option<FinalityEvidence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseInstruction {
    pub withdrawal: WithdrawalId,
    pub asset: AssetId,
    pub origin_network: OriginNetworkId,
    pub origin_asset: OriginAssetId,
    pub amount: Balance,
    pub destination: DestinationCommitment,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AssetSettlementAudit {
    pub finalized_deposits: Balance,
    pub minted: Balance,
    pub burned: Balance,
    pub completed_releases: Balance,
    pub expected_backing: Balance,
    pub represented_supply: Balance,
    pub backing_surplus: Balance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalAssetEvent {
    AssetRegistered {
        asset: AssetId,
    },
    StatusChanged {
        asset: AssetId,
        status: ConnectorStatus,
    },
    DepositObserved {
        asset: AssetId,
        deposit: DepositId,
    },
    DepositFinalized {
        asset: AssetId,
        deposit: DepositId,
    },
    MintAuthorized {
        asset: AssetId,
        deposit: DepositId,
    },
    MintRecorded {
        asset: AssetId,
        deposit: DepositId,
    },
    WithdrawalRecorded {
        asset: AssetId,
        withdrawal: WithdrawalId,
    },
    ReleaseAuthorized {
        asset: AssetId,
        withdrawal: WithdrawalId,
    },
    ReleaseSubmitted {
        asset: AssetId,
        withdrawal: WithdrawalId,
    },
    WithdrawalCompleted {
        asset: AssetId,
        withdrawal: WithdrawalId,
    },
}
