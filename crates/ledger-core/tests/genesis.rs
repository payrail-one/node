use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement, GenesisAsset,
    GenesisBalance, GenesisConfig, Ledger, LedgerError, NetworkId,
};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const BOB: AccountId = AccountId::new([7; 32]);
const NETWORK_TOKEN: AssetId = AssetId::new([11; 32]);
const EXTERNAL_ASSET: AssetId = AssetId::new([12; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);

fn definition(
    id: AssetId,
    symbol: &str,
    class: AssetClass,
    backing_requirement: BackingRequirement,
) -> AssetDefinition {
    AssetDefinition {
        id,
        symbol: symbol.to_owned(),
        decimals: 6,
        class,
        status: AssetStatus::Active,
        issuer: ISSUER,
        backing_authority: BACKING,
        freeze_authority: FREEZER,
        treasury: TREASURY,
        max_supply: Some(1_000),
        backing_requirement,
    }
}

fn canonical_config() -> GenesisConfig {
    GenesisConfig {
        network: NETWORK,
        registry_authority: REGISTRY,
        assets: vec![
            GenesisAsset {
                definition: definition(
                    NETWORK_TOKEN,
                    "PAY",
                    AssetClass::NetworkNative,
                    BackingRequirement::None,
                ),
                verified_backing: 0,
            },
            GenesisAsset {
                definition: definition(
                    EXTERNAL_ASSET,
                    "USDT",
                    AssetClass::ExternalRepresentation,
                    BackingRequirement::VerifiedOneToOne,
                ),
                verified_backing: 500,
            },
        ],
        balances: vec![
            GenesisBalance {
                asset: NETWORK_TOKEN,
                account: ALICE,
                amount: 200,
            },
            GenesisBalance {
                asset: NETWORK_TOKEN,
                account: BOB,
                amount: 100,
            },
            GenesisBalance {
                asset: EXTERNAL_ASSET,
                account: ALICE,
                amount: 400,
            },
        ],
    }
}

#[test]
fn canonical_genesis_is_deterministic_and_reconciled() {
    let first = Ledger::from_genesis(canonical_config()).unwrap();
    let second = Ledger::from_genesis(canonical_config()).unwrap();

    assert_eq!(first.events(), second.events());
    assert_eq!(first.balance(NETWORK_TOKEN, ALICE), 200);
    assert_eq!(first.balance(NETWORK_TOKEN, BOB), 100);
    assert_eq!(first.balance(EXTERNAL_ASSET, ALICE), 400);

    let network_audit = first.audit_asset(NETWORK_TOKEN).unwrap();
    assert_eq!(network_audit.declared_supply, 300);
    assert!(network_audit.supply_matches_balances);

    let external_audit = first.audit_asset(EXTERNAL_ASSET).unwrap();
    assert_eq!(external_audit.declared_supply, 400);
    assert_eq!(external_audit.verified_backing, 500);
    assert!(external_audit.supply_matches_balances);
    assert!(external_audit.backing_covers_supply);
}

#[test]
fn assets_must_be_strictly_ordered_and_unique() {
    let mut config = canonical_config();
    config.assets.swap(0, 1);
    assert_eq!(
        Ledger::from_genesis(config).unwrap_err(),
        LedgerError::NonCanonicalGenesisAssets
    );

    let mut duplicate = canonical_config();
    duplicate.assets[1].definition.id = NETWORK_TOKEN;
    assert_eq!(
        Ledger::from_genesis(duplicate).unwrap_err(),
        LedgerError::NonCanonicalGenesisAssets
    );
}

#[test]
fn balances_must_be_strictly_ordered_and_unique() {
    let mut config = canonical_config();
    config.balances.swap(0, 1);
    assert_eq!(
        Ledger::from_genesis(config).unwrap_err(),
        LedgerError::NonCanonicalGenesisBalances
    );

    let mut duplicate = canonical_config();
    duplicate.balances[1].account = ALICE;
    assert_eq!(
        Ledger::from_genesis(duplicate).unwrap_err(),
        LedgerError::NonCanonicalGenesisBalances
    );
}

#[test]
fn genesis_cannot_create_unbacked_external_supply() {
    let mut config = canonical_config();
    config.balances[2].amount = 501;

    assert_eq!(
        Ledger::from_genesis(config).unwrap_err(),
        LedgerError::BackingExceeded
    );
}

#[test]
fn genesis_enforces_supply_cap() {
    let mut config = canonical_config();
    config.balances[1].amount = 801;

    assert_eq!(
        Ledger::from_genesis(config).unwrap_err(),
        LedgerError::SupplyCapExceeded
    );
}

#[test]
fn unbacked_asset_cannot_declare_genesis_backing() {
    let mut config = canonical_config();
    config.assets[0].verified_backing = 1;

    assert_eq!(
        Ledger::from_genesis(config).unwrap_err(),
        LedgerError::UnexpectedGenesisBacking
    );
}
