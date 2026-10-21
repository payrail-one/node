use external_asset_core::{
    ConnectorStatus, ExternalAssetConfig, ExternalAssetCoordinator, FinalityEvidence,
    OriginAssetId, OriginDeposit, OriginNetworkId, OriginTransactionId, SettlementReference,
};
use external_asset_runtime_core::{
    ExternalSettlementStateCodec, ExternalSettlementStateCodecError,
};
use external_asset_settlement_core::ExternalAssetSettlement;
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement, Ledger,
    NetworkId,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const REGISTRY: AccountId = AccountId::new([2; 32]);
const OBSERVER: AccountId = AccountId::new([3; 32]);
const FINALITY: AccountId = AccountId::new([4; 32]);
const SETTLEMENT: AccountId = AccountId::new([5; 32]);
const RELEASE: AccountId = AccountId::new([6; 32]);
const ASSET: AssetId = AssetId::new([7; 32]);

fn state() -> ExternalAssetSettlement {
    let mut state = ExternalAssetSettlement::new(
        Ledger::new(NETWORK, REGISTRY),
        ExternalAssetCoordinator::new(REGISTRY),
    );
    state
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: ASSET,
                symbol: "WETH".to_owned(),
                decimals: 18,
                class: AssetClass::ExternalRepresentation,
                status: AssetStatus::Test,
                issuer: SETTLEMENT,
                backing_authority: FINALITY,
                freeze_authority: AccountId::new([8; 32]),
                treasury: AccountId::new([9; 32]),
                max_supply: None,
                backing_requirement: BackingRequirement::VerifiedOneToOne,
            },
            ExternalAssetConfig {
                asset: ASSET,
                origin_network: OriginNetworkId::new([10; 32]),
                origin_asset: OriginAssetId::new([11; 32]),
                observer_authority: OBSERVER,
                finality_authority: FINALITY,
                settlement_authority: SETTLEMENT,
                release_authority: RELEASE,
                status: ConnectorStatus::Active,
            },
        )
        .unwrap();
    state
        .settle_deposit(
            OBSERVER,
            FINALITY,
            SETTLEMENT,
            OriginDeposit {
                asset: ASSET,
                origin_network: OriginNetworkId::new([10; 32]),
                origin_asset: OriginAssetId::new([11; 32]),
                transaction: OriginTransactionId::new([12; 32]),
                event_index: 1,
                recipient: AccountId::new([13; 32]),
                amount: 500,
            },
            FinalityEvidence {
                block_height: 100,
                block_hash: [14; 32],
                proof_hash: [15; 32],
            },
            SettlementReference::new([16; 32]),
        )
        .unwrap();
    state
}

#[test]
fn combined_state_round_trips_exactly() {
    let snapshot = state().snapshot();
    let encoded = ExternalSettlementStateCodec::encode(&snapshot).unwrap();
    assert_eq!(
        ExternalSettlementStateCodec::decode(NETWORK, &encoded),
        Ok(snapshot)
    );
}

#[test]
fn framing_rejects_every_truncation_trailing_bytes_and_wrong_network() {
    let encoded = ExternalSettlementStateCodec::encode(&state().snapshot()).unwrap();
    for end in 0..encoded.len() {
        assert!(ExternalSettlementStateCodec::decode(NETWORK, &encoded[..end]).is_err());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert_eq!(
        ExternalSettlementStateCodec::decode(NETWORK, &trailing),
        Err(ExternalSettlementStateCodecError::TrailingBytes)
    );
    assert!(ExternalSettlementStateCodec::decode(NetworkId::new([99; 32]), &encoded).is_err());
}
