use account_address::AddressCodec;
use ethereum_bridge_core::{EvmAddress, EvmAsset, EvmChainId, origin_asset_id, origin_network_id};
use external_asset_core::{ConnectorStatus, ExternalAssetConfig};
use external_asset_runtime_core::{
    ExternalSettlementExecutionKind, ExternalSettlementRuntimeConfig,
};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement,
};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::FinalizedTransactionView,
    service::NETWORK,
};

pub(crate) const BRIDGE_ASSET: AssetId = AssetId::new([
    0xb3, 0x56, 0x7d, 0x3b, 0x15, 0x07, 0xf7, 0xb2, 0x0c, 0x31, 0xea, 0x7a, 0x48, 0xaa, 0x92, 0xe1,
    0x6d, 0x80, 0x8a, 0x77, 0x67, 0x4b, 0xef, 0x63, 0x8b, 0x97, 0xec, 0xaa, 0x90, 0x9f, 0xc9, 0x0b,
]);
pub(crate) const BRIDGE_SYMBOL: &str = "TUSDT";
pub(crate) const BRIDGE_DECIMALS: u8 = 6;
const TEST_USDT_SUPPLY: u128 = 1_000_000 * 1_000_000;
const SEPOLIA_CHAIN_ID: u64 = 11_155_111;
const SEPOLIA_TEST_USDT: EvmAddress = EvmAddress::new([
    0x5e, 0x3a, 0x1b, 0xbb, 0x81, 0xd5, 0xc3, 0xc1, 0x39, 0x17, 0x96, 0xe4, 0xe3, 0xc8, 0xc4, 0xe0,
    0x42, 0x0a, 0x8c, 0x4a,
]);

pub(crate) fn runtime_config() -> Result<ExternalSettlementRuntimeConfig, DevnetError> {
    // The running public devnet genesis fixes this registry authority. Asset
    // registration is a deterministic activation migration, not a hot-key call.
    let registry = AccountId::new([52; 32]);
    let observer = authority("DEVNET_BRIDGE_OBSERVER_AUTHORITY", [62; 32])?;
    let finality = authority("DEVNET_BRIDGE_FINALITY_AUTHORITY", [63; 32])?;
    let settlement = authority("DEVNET_BRIDGE_SETTLEMENT_AUTHORITY", [64; 32])?;
    let release = authority("DEVNET_BRIDGE_RELEASE_AUTHORITY", [65; 32])?;
    let chain = EvmChainId::new(SEPOLIA_CHAIN_ID).ok_or(DevnetError::InternalInvariant)?;
    Ok(ExternalSettlementRuntimeConfig {
        network: NETWORK,
        activation_height: activation_height()?,
        registry_authority: registry,
        asset_definition: AssetDefinition {
            id: BRIDGE_ASSET,
            symbol: BRIDGE_SYMBOL.to_owned(),
            decimals: BRIDGE_DECIMALS,
            class: AssetClass::ExternalRepresentation,
            status: AssetStatus::Test,
            issuer: settlement,
            backing_authority: finality,
            freeze_authority: registry,
            treasury: registry,
            max_supply: Some(TEST_USDT_SUPPLY),
            backing_requirement: BackingRequirement::VerifiedOneToOne,
        },
        external_asset: ExternalAssetConfig {
            asset: BRIDGE_ASSET,
            origin_network: origin_network_id(chain),
            origin_asset: origin_asset_id(chain, EvmAsset::Erc20(SEPOLIA_TEST_USDT)),
            observer_authority: observer,
            finality_authority: finality,
            settlement_authority: settlement,
            release_authority: release,
            status: ConnectorStatus::Active,
        },
    })
}

fn activation_height() -> Result<u64, DevnetError> {
    match std::env::var("DEVNET_BRIDGE_ACTIVATION_HEIGHT") {
        Ok(value) if !value.is_empty() => value
            .parse::<u64>()
            .ok()
            .filter(|height| *height > 0)
            .ok_or(DevnetError::InvalidQuorumConfiguration),
        Ok(_) | Err(_) => Ok(1),
    }
}

fn authority(name: &str, default: [u8; 32]) -> Result<AccountId, DevnetError> {
    let Some(encoded) = std::env::var(name).ok().filter(|value| !value.is_empty()) else {
        return Ok(AccountId::new(default));
    };
    decode_bounded_hex(&encoded, 32)?
        .try_into()
        .map(AccountId::new)
        .map_err(|_| DevnetError::InvalidQuorumConfiguration)
}

pub(crate) fn bridge_transaction_view(
    addresses: &AddressCodec,
    checkpoint: FinalizedCheckpoint,
    kind: &ExternalSettlementExecutionKind,
    settlement: &external_asset_settlement_core::ExternalAssetSettlement,
) -> Result<FinalizedTransactionView, DevnetError> {
    let config = settlement
        .external()
        .asset_config(BRIDGE_ASSET)
        .ok_or(DevnetError::InternalInvariant)?;
    let (from, to, amount, operation_kind) = match kind {
        ExternalSettlementExecutionKind::Deposit(deposit) => {
            let record = settlement
                .external()
                .deposit_record(*deposit)
                .ok_or(DevnetError::InternalInvariant)?;
            (
                config.observer_authority,
                record.claim.recipient,
                record.claim.amount,
                "bridgeDeposit",
            )
        }
        ExternalSettlementExecutionKind::Withdrawal { withdrawal, .. } => {
            let record = settlement
                .external()
                .withdrawal_record(*withdrawal)
                .ok_or(DevnetError::InternalInvariant)?;
            (
                record.request.account,
                config.release_authority,
                record.request.amount,
                "bridgeWithdrawal",
            )
        }
        ExternalSettlementExecutionKind::CompleteWithdrawal(withdrawal) => {
            let record = settlement
                .external()
                .withdrawal_record(*withdrawal)
                .ok_or(DevnetError::InternalInvariant)?;
            (
                config.release_authority,
                record.request.account,
                record.request.amount,
                "bridgeReleaseFinalized",
            )
        }
        ExternalSettlementExecutionKind::Ledger(_) => {
            return Err(DevnetError::InternalInvariant);
        }
    };
    Ok(FinalizedTransactionView {
        id: encode_hex(checkpoint.block_hash.as_bytes()),
        block_height: checkpoint.height.to_string(),
        operation_index: "0".to_owned(),
        from: addresses
            .encode(from)
            .map_err(|_| DevnetError::InternalInvariant)?,
        to: addresses
            .encode(to)
            .map_err(|_| DevnetError::InternalInvariant)?,
        amount: amount.to_string(),
        fee: "0".to_owned(),
        outcome: "applied",
        kind: operation_kind,
    })
}
