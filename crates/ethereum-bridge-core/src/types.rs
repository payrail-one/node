use external_asset_core::{FinalityEvidence, OriginDeposit};
use ledger_core::AssetId;

macro_rules! byte_identifier {
    ($name:ident, $size:expr) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; $size]);

        impl $name {
            #[must_use]
            pub const fn new(value: [u8; $size]) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $size] {
                &self.0
            }

            #[must_use]
            pub fn is_zero(self) -> bool {
                self.0.iter().all(|byte| *byte == 0)
            }
        }
    };
}

byte_identifier!(EvmAddress, 20);
byte_identifier!(EvmHash, 32);
byte_identifier!(EvmProviderId, 32);

/// `keccak256("Deposited(bytes32,address,uint256,uint256)")`.
pub const DEPOSITED_EVENT_TOPIC: EvmHash = EvmHash::new([
    0x2d, 0xcd, 0xaa, 0xd8, 0x7b, 0x56, 0x1b, 0xa5, 0xa6, 0x98, 0x35, 0x00, 0x9b, 0x4c, 0x53, 0xef,
    0x9d, 0x3c, 0x41, 0xca, 0x6c, 0xc9, 0x57, 0x40, 0x49, 0x18, 0x76, 0x59, 0xd6, 0xc6, 0xa7, 0x15,
]);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvmChainId(u64);

impl EvmChainId {
    #[must_use]
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmAsset {
    Native,
    Erc20(EvmAddress),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmAssetBinding {
    chain_id: EvmChainId,
    bridge_contract: EvmAddress,
    origin_asset: EvmAsset,
    payrail_asset: AssetId,
}

impl EvmAssetBinding {
    /// Creates an exact Payrail-to-Ethereum asset binding.
    ///
    /// # Errors
    ///
    /// Returns [`crate::EvmDepositError::ZeroBridgeContract`] for the zero
    /// bridge address or [`crate::EvmDepositError::ZeroTokenContract`] when an
    /// ERC-20 binding uses the native-asset sentinel.
    pub fn new(
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        origin_asset: EvmAsset,
        payrail_asset: AssetId,
    ) -> Result<Self, crate::EvmDepositError> {
        if bridge_contract.is_zero() {
            return Err(crate::EvmDepositError::ZeroBridgeContract);
        }
        if matches!(origin_asset, EvmAsset::Erc20(address) if address.is_zero()) {
            return Err(crate::EvmDepositError::ZeroTokenContract);
        }
        Ok(Self {
            chain_id,
            bridge_contract,
            origin_asset,
            payrail_asset,
        })
    }

    #[must_use]
    pub const fn chain_id(self) -> EvmChainId {
        self.chain_id
    }

    #[must_use]
    pub const fn bridge_contract(self) -> EvmAddress {
        self.bridge_contract
    }

    #[must_use]
    pub const fn origin_asset(self) -> EvmAsset {
        self.origin_asset
    }

    #[must_use]
    pub const fn payrail_asset(self) -> AssetId {
        self.payrail_asset
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvmLog {
    pub address: EvmAddress,
    pub topics: Vec<EvmHash>,
    pub data: Vec<u8>,
    pub transaction_hash: Option<EvmHash>,
    pub block_hash: Option<EvmHash>,
    pub block_number: Option<u64>,
    pub log_index: Option<u64>,
    pub removed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvmObservedBlock {
    pub number: u64,
    pub hash: EvmHash,
    pub parent_hash: EvmHash,
    pub logs: Vec<EvmLog>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedEvmDeposit {
    pub(crate) claim: OriginDeposit,
    pub(crate) block_hash: EvmHash,
    pub(crate) block_number: u64,
    pub(crate) deposit_nonce: [u8; 32],
}

impl ObservedEvmDeposit {
    #[must_use]
    pub const fn claim(self) -> OriginDeposit {
        self.claim
    }

    #[must_use]
    pub const fn block_hash(self) -> EvmHash {
        self.block_hash
    }

    #[must_use]
    pub const fn block_number(self) -> u64 {
        self.block_number
    }

    #[must_use]
    pub const fn deposit_nonce(self) -> [u8; 32] {
        self.deposit_nonce
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmFinalizedBlock {
    pub number: u64,
    pub hash: EvmHash,
    /// Commitment to the independently verified source data used to establish
    /// canonical finality. It must not contain credentials or raw RPC payloads.
    pub proof_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmProviderFinality {
    pub block_number: u64,
    pub block_hash: EvmHash,
    pub finalized_head_number: u64,
    pub finalized_head_hash: EvmHash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedEvmDeposit {
    pub(crate) claim: OriginDeposit,
    pub(crate) evidence: FinalityEvidence,
}

impl VerifiedEvmDeposit {
    #[must_use]
    pub const fn claim(self) -> OriginDeposit {
        self.claim
    }

    #[must_use]
    pub const fn evidence(self) -> FinalityEvidence {
        self.evidence
    }

    #[must_use]
    pub const fn into_parts(self) -> (OriginDeposit, FinalityEvidence) {
        (self.claim, self.evidence)
    }
}
