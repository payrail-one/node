use external_asset_core::{OriginAssetId, OriginNetworkId};
use sha2::{Digest, Sha256};

use crate::{EvmAsset, EvmChainId};

#[must_use]
pub fn origin_network_id(chain_id: EvmChainId) -> OriginNetworkId {
    let mut hasher = Sha256::new();
    hasher.update(b"payrail.ethereum.origin-network\0");
    hasher.update(chain_id.get().to_be_bytes());
    OriginNetworkId::new(hasher.finalize().into())
}

#[must_use]
pub fn origin_asset_id(chain_id: EvmChainId, asset: EvmAsset) -> OriginAssetId {
    let mut hasher = Sha256::new();
    hasher.update(b"payrail.ethereum.origin-asset\0");
    hasher.update(chain_id.get().to_be_bytes());
    match asset {
        EvmAsset::Native => hasher.update([0]),
        EvmAsset::Erc20(address) => {
            hasher.update([1]);
            hasher.update(address.as_bytes());
        }
    }
    OriginAssetId::new(hasher.finalize().into())
}
