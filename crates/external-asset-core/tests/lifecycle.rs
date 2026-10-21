use external_asset_core::{
    AssetSettlementAudit, ConnectorStatus, DepositId, DepositStatus, DestinationCommitment,
    ExternalAssetConfig, ExternalAssetCoordinator, ExternalAssetError, ExternalAssetStateCodec,
    ExternalAssetStateCodecError, FinalityEvidence, OriginAssetId, OriginDeposit, OriginNetworkId,
    OriginReleaseReference, OriginTransactionId, SettlementReference, WithdrawalId,
    WithdrawalRequest, WithdrawalStatus,
};
use ledger_core::{AccountId, AssetId};

const REGISTRY: AccountId = AccountId::new([1; 32]);
const OBSERVER: AccountId = AccountId::new([2; 32]);
const FINALITY: AccountId = AccountId::new([3; 32]);
const SETTLEMENT: AccountId = AccountId::new([4; 32]);
const RELEASE: AccountId = AccountId::new([5; 32]);
const ALICE: AccountId = AccountId::new([6; 32]);
const UNKNOWN: AccountId = AccountId::new([99; 32]);
const ASSET: AssetId = AssetId::new([10; 32]);
const SECOND_ASSET: AssetId = AssetId::new([20; 32]);
const ORIGIN_NETWORK: OriginNetworkId = OriginNetworkId::new([11; 32]);
const ORIGIN_ASSET: OriginAssetId = OriginAssetId::new([12; 32]);
const ORIGIN_TRANSACTION: OriginTransactionId = OriginTransactionId::new([13; 32]);
const MINT_REFERENCE: SettlementReference = SettlementReference::new([14; 32]);
const BURN_REFERENCE: SettlementReference = SettlementReference::new([15; 32]);
const DESTINATION: DestinationCommitment = DestinationCommitment::new([16; 32]);
const RELEASE_TRANSACTION: OriginTransactionId = OriginTransactionId::new([17; 32]);

fn release_reference(event_index: u32) -> OriginReleaseReference {
    OriginReleaseReference {
        transaction: RELEASE_TRANSACTION,
        event_index,
    }
}

fn config(asset: AssetId, origin_asset: OriginAssetId) -> ExternalAssetConfig {
    ExternalAssetConfig {
        asset,
        origin_network: ORIGIN_NETWORK,
        origin_asset,
        observer_authority: OBSERVER,
        finality_authority: FINALITY,
        settlement_authority: SETTLEMENT,
        release_authority: RELEASE,
        status: ConnectorStatus::Active,
    }
}

fn claim(transaction: OriginTransactionId, event_index: u32, amount: u128) -> OriginDeposit {
    OriginDeposit {
        asset: ASSET,
        origin_network: ORIGIN_NETWORK,
        origin_asset: ORIGIN_ASSET,
        transaction,
        event_index,
        recipient: ALICE,
        amount,
    }
}

fn finality(byte: u8, height: u64) -> FinalityEvidence {
    FinalityEvidence {
        block_height: height,
        block_hash: [byte; 32],
        proof_hash: [byte.wrapping_add(1); 32],
    }
}

fn coordinator() -> ExternalAssetCoordinator {
    let mut coordinator = ExternalAssetCoordinator::new(REGISTRY);
    coordinator
        .register_asset(REGISTRY, config(ASSET, ORIGIN_ASSET))
        .unwrap();
    coordinator
}

fn mint_deposit(
    coordinator: &mut ExternalAssetCoordinator,
    deposit_claim: OriginDeposit,
    reference: SettlementReference,
) -> DepositId {
    let deposit = coordinator
        .observe_deposit(OBSERVER, deposit_claim)
        .unwrap();
    coordinator
        .finalize_deposit(FINALITY, deposit, finality(30, 100))
        .unwrap();
    coordinator.authorize_mint(SETTLEMENT, deposit).unwrap();
    coordinator
        .record_mint(SETTLEMENT, deposit, reference)
        .unwrap();
    deposit
}

#[test]
fn deposit_to_mint_and_burn_to_release_reconcile() {
    let mut coordinator = coordinator();
    let deposit = coordinator
        .observe_deposit(OBSERVER, claim(ORIGIN_TRANSACTION, 0, 1_000))
        .unwrap();
    assert_eq!(
        coordinator.deposit_record(deposit).unwrap().status,
        DepositStatus::Observed
    );

    coordinator
        .finalize_deposit(FINALITY, deposit, finality(30, 100))
        .unwrap();
    assert_eq!(
        coordinator.audit(ASSET).unwrap(),
        AssetSettlementAudit {
            finalized_deposits: 1_000,
            expected_backing: 1_000,
            backing_surplus: 1_000,
            ..AssetSettlementAudit::default()
        }
    );

    let mint = coordinator.authorize_mint(SETTLEMENT, deposit).unwrap();
    assert_eq!(mint.deposit, deposit);
    assert_eq!(mint.asset, ASSET);
    assert_eq!(mint.recipient, ALICE);
    assert_eq!(mint.amount, 1_000);
    coordinator
        .record_mint(SETTLEMENT, deposit, MINT_REFERENCE)
        .unwrap();

    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 300,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let withdrawal = coordinator.record_burn(SETTLEMENT, request).unwrap();
    assert_eq!(WithdrawalId::derive(&request), withdrawal);
    assert_eq!(
        coordinator.audit(ASSET).unwrap(),
        AssetSettlementAudit {
            finalized_deposits: 1_000,
            minted: 1_000,
            burned: 300,
            expected_backing: 1_000,
            represented_supply: 700,
            backing_surplus: 300,
            ..AssetSettlementAudit::default()
        }
    );

    let release = coordinator.authorize_release(RELEASE, withdrawal).unwrap();
    assert_eq!(release.withdrawal, withdrawal);
    assert_eq!(release.origin_network, ORIGIN_NETWORK);
    assert_eq!(release.origin_asset, ORIGIN_ASSET);
    assert_eq!(release.amount, 300);
    assert_eq!(release.destination, DESTINATION);
    coordinator
        .record_release_submission(RELEASE, withdrawal, release_reference(0))
        .unwrap();
    coordinator
        .complete_withdrawal(FINALITY, withdrawal, finality(40, 200))
        .unwrap();

    assert_eq!(
        coordinator.withdrawal_record(withdrawal).unwrap().status,
        WithdrawalStatus::Completed
    );
    assert_eq!(
        coordinator.audit(ASSET).unwrap(),
        AssetSettlementAudit {
            finalized_deposits: 1_000,
            minted: 1_000,
            burned: 300,
            completed_releases: 300,
            expected_backing: 700,
            represented_supply: 700,
            backing_surplus: 0,
        }
    );
    assert_eq!(coordinator.events().len(), 9);
}

#[test]
fn exact_deposit_retries_are_idempotent_but_conflicts_fail() {
    let mut coordinator = coordinator();
    let original = claim(ORIGIN_TRANSACTION, 7, 500);
    let deposit = coordinator.observe_deposit(OBSERVER, original).unwrap();

    assert_eq!(coordinator.observe_deposit(OBSERVER, original), Ok(deposit));
    let mut conflict = original;
    conflict.amount = 501;
    assert_eq!(
        coordinator.observe_deposit(OBSERVER, conflict),
        Err(ExternalAssetError::DepositReferenceConflict)
    );
    assert_eq!(coordinator.events().len(), 2);
}

#[test]
fn mint_requires_finality_and_cannot_be_authorized_twice_after_execution() {
    let mut coordinator = coordinator();
    let deposit = coordinator
        .observe_deposit(OBSERVER, claim(ORIGIN_TRANSACTION, 0, 100))
        .unwrap();

    assert_eq!(
        coordinator.authorize_mint(SETTLEMENT, deposit),
        Err(ExternalAssetError::InvalidTransition)
    );
    coordinator
        .finalize_deposit(FINALITY, deposit, finality(30, 100))
        .unwrap();
    assert!(coordinator.authorize_mint(SETTLEMENT, deposit).is_ok());
    assert!(coordinator.authorize_mint(SETTLEMENT, deposit).is_ok());
    coordinator
        .record_mint(SETTLEMENT, deposit, MINT_REFERENCE)
        .unwrap();
    assert_eq!(
        coordinator.authorize_mint(SETTLEMENT, deposit),
        Err(ExternalAssetError::InvalidTransition)
    );
}

#[test]
fn settlement_references_are_unique_and_retries_must_match() {
    let mut coordinator = coordinator();
    let first = mint_deposit(
        &mut coordinator,
        claim(ORIGIN_TRANSACTION, 0, 100),
        MINT_REFERENCE,
    );
    assert_eq!(
        coordinator.record_mint(SETTLEMENT, first, MINT_REFERENCE),
        Ok(())
    );
    assert_eq!(
        coordinator.record_mint(SETTLEMENT, first, SettlementReference::new([88; 32])),
        Err(ExternalAssetError::SettlementReferenceConflict)
    );

    let second_claim = claim(OriginTransactionId::new([18; 32]), 0, 100);
    let second = coordinator.observe_deposit(OBSERVER, second_claim).unwrap();
    coordinator
        .finalize_deposit(FINALITY, second, finality(31, 101))
        .unwrap();
    coordinator.authorize_mint(SETTLEMENT, second).unwrap();
    assert_eq!(
        coordinator.record_mint(SETTLEMENT, second, MINT_REFERENCE),
        Err(ExternalAssetError::SettlementReferenceConflict)
    );
    assert_eq!(
        coordinator.deposit_record(second).unwrap().status,
        DepositStatus::MintAuthorized
    );
}

#[test]
fn withdrawal_cannot_exceed_recorded_mints() {
    let mut coordinator = coordinator();
    mint_deposit(
        &mut coordinator,
        claim(ORIGIN_TRANSACTION, 0, 100),
        MINT_REFERENCE,
    );
    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 101,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };

    assert_eq!(
        coordinator.record_burn(SETTLEMENT, request),
        Err(ExternalAssetError::InsufficientMintedSupply)
    );
    assert_eq!(coordinator.audit(ASSET).unwrap().burned, 0);
}

#[test]
fn burn_and_origin_release_references_cannot_be_reused() {
    let mut coordinator = coordinator();
    mint_deposit(
        &mut coordinator,
        claim(ORIGIN_TRANSACTION, 0, 200),
        MINT_REFERENCE,
    );
    let first_request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 50,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let first = coordinator.record_burn(SETTLEMENT, first_request).unwrap();
    assert_eq!(
        coordinator.record_burn(SETTLEMENT, first_request),
        Ok(first)
    );

    let mut conflict = first_request;
    conflict.amount = 51;
    assert_eq!(
        coordinator.record_burn(SETTLEMENT, conflict),
        Err(ExternalAssetError::BurnReferenceConflict)
    );
    coordinator.authorize_release(RELEASE, first).unwrap();
    coordinator
        .record_release_submission(RELEASE, first, release_reference(0))
        .unwrap();

    let second_request = WithdrawalRequest {
        amount: 50,
        burn_reference: SettlementReference::new([19; 32]),
        ..first_request
    };
    let second = coordinator.record_burn(SETTLEMENT, second_request).unwrap();
    coordinator.authorize_release(RELEASE, second).unwrap();
    assert_eq!(
        coordinator.record_release_submission(RELEASE, second, release_reference(0)),
        Err(ExternalAssetError::OriginTransactionConflict)
    );
    assert_eq!(
        coordinator.record_release_submission(RELEASE, second, release_reference(1)),
        Ok(())
    );
}

#[test]
fn completed_or_submitted_withdrawal_cannot_emit_another_release_instruction() {
    let mut coordinator = coordinator();
    mint_deposit(
        &mut coordinator,
        claim(ORIGIN_TRANSACTION, 0, 100),
        MINT_REFERENCE,
    );
    let withdrawal = coordinator
        .record_burn(
            SETTLEMENT,
            WithdrawalRequest {
                asset: ASSET,
                account: ALICE,
                amount: 50,
                destination: DESTINATION,
                burn_reference: BURN_REFERENCE,
            },
        )
        .unwrap();
    coordinator.authorize_release(RELEASE, withdrawal).unwrap();
    assert!(coordinator.authorize_release(RELEASE, withdrawal).is_ok());
    coordinator
        .record_release_submission(RELEASE, withdrawal, release_reference(0))
        .unwrap();

    assert_eq!(
        coordinator.authorize_release(RELEASE, withdrawal),
        Err(ExternalAssetError::InvalidTransition)
    );
}

#[test]
fn role_separation_and_origin_mapping_are_enforced() {
    let mut coordinator = coordinator();
    let deposit_claim = claim(ORIGIN_TRANSACTION, 0, 100);
    assert_eq!(
        coordinator.observe_deposit(UNKNOWN, deposit_claim),
        Err(ExternalAssetError::Unauthorized)
    );

    let mut wrong_origin = deposit_claim;
    wrong_origin.origin_network = OriginNetworkId::new([44; 32]);
    assert_eq!(
        coordinator.observe_deposit(OBSERVER, wrong_origin),
        Err(ExternalAssetError::InvalidDepositOrigin)
    );

    assert_eq!(
        coordinator.register_asset(REGISTRY, config(SECOND_ASSET, OriginAssetId::new([21; 32]))),
        Ok(())
    );
    assert_eq!(
        coordinator.register_asset(REGISTRY, config(AssetId::new([22; 32]), ORIGIN_ASSET)),
        Err(ExternalAssetError::OriginAssetAlreadyRegistered)
    );
}

#[test]
fn pause_stops_new_risk_but_allows_irreversible_work_to_be_reconciled() {
    let mut coordinator = coordinator();
    let deposit = coordinator
        .observe_deposit(OBSERVER, claim(ORIGIN_TRANSACTION, 0, 100))
        .unwrap();
    coordinator
        .finalize_deposit(FINALITY, deposit, finality(30, 100))
        .unwrap();
    coordinator.authorize_mint(SETTLEMENT, deposit).unwrap();
    coordinator
        .set_status(REGISTRY, ASSET, ConnectorStatus::Suspended)
        .unwrap();

    assert_eq!(
        coordinator.observe_deposit(OBSERVER, claim(OriginTransactionId::new([22; 32]), 0, 100)),
        Err(ExternalAssetError::DepositsPaused)
    );
    assert_eq!(
        coordinator.record_burn(
            SETTLEMENT,
            WithdrawalRequest {
                asset: ASSET,
                account: ALICE,
                amount: 1,
                destination: DESTINATION,
                burn_reference: BURN_REFERENCE,
            }
        ),
        Err(ExternalAssetError::WithdrawalsPaused)
    );
    assert_eq!(
        coordinator.record_mint(SETTLEMENT, deposit, MINT_REFERENCE),
        Ok(())
    );
}

#[test]
fn identifier_derivation_is_stable_and_separates_origin_events() {
    let first = claim(ORIGIN_TRANSACTION, 0, 100);
    let mut second = first;
    second.event_index = 1;
    let withdrawal = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 50,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };

    assert_ne!(DepositId::derive(&first), DepositId::derive(&second));
    assert_eq!(
        DepositId::derive(&first).as_bytes(),
        &[
            104, 137, 162, 86, 158, 56, 209, 225, 255, 20, 28, 9, 78, 124, 76, 210, 62, 175, 71,
            234, 209, 18, 96, 9, 62, 47, 10, 86, 211, 1, 9, 211,
        ]
    );
    assert_eq!(
        WithdrawalId::derive(&withdrawal).as_bytes(),
        &[
            89, 119, 2, 68, 219, 43, 70, 188, 118, 122, 167, 77, 245, 91, 229, 235, 202, 131, 249,
            245, 181, 229, 61, 192, 243, 219, 233, 166, 40, 109, 193, 77,
        ]
    );
}

#[test]
fn authority_roles_must_be_nonzero_and_separated() {
    let mut coordinator = ExternalAssetCoordinator::new(REGISTRY);
    let mut collapsed = config(ASSET, ORIGIN_ASSET);
    collapsed.finality_authority = collapsed.observer_authority;
    assert_eq!(
        coordinator.register_asset(REGISTRY, collapsed),
        Err(ExternalAssetError::InvalidAuthorityConfiguration)
    );

    let mut zero = config(ASSET, ORIGIN_ASSET);
    zero.release_authority = AccountId::new([0; 32]);
    assert_eq!(
        coordinator.register_asset(REGISTRY, zero),
        Err(ExternalAssetError::InvalidAuthorityConfiguration)
    );
}

#[test]
fn zero_evidence_references_and_destination_fail_before_progress() {
    let mut coordinator = coordinator();
    let deposit = coordinator
        .observe_deposit(OBSERVER, claim(ORIGIN_TRANSACTION, 0, 100))
        .unwrap();
    assert_eq!(
        coordinator.finalize_deposit(
            FINALITY,
            deposit,
            FinalityEvidence {
                block_height: 1,
                block_hash: [0; 32],
                proof_hash: [1; 32],
            },
        ),
        Err(ExternalAssetError::InvalidFinalityEvidence)
    );

    coordinator
        .finalize_deposit(FINALITY, deposit, finality(1, 1))
        .unwrap();
    coordinator.authorize_mint(SETTLEMENT, deposit).unwrap();
    assert_eq!(
        coordinator.record_mint(SETTLEMENT, deposit, SettlementReference::new([0; 32])),
        Err(ExternalAssetError::InvalidSettlementReference)
    );

    coordinator
        .record_mint(SETTLEMENT, deposit, MINT_REFERENCE)
        .unwrap();
    assert_eq!(
        coordinator.record_burn(
            SETTLEMENT,
            WithdrawalRequest {
                asset: ASSET,
                account: ALICE,
                amount: 1,
                destination: DestinationCommitment::new([0; 32]),
                burn_reference: BURN_REFERENCE,
            },
        ),
        Err(ExternalAssetError::InvalidDestination)
    );
}

#[test]
fn canonical_snapshot_round_trip_rebuilds_indexes_totals_and_no_events() {
    let mut original = coordinator();
    let deposit = mint_deposit(
        &mut original,
        claim(ORIGIN_TRANSACTION, 0, 200),
        MINT_REFERENCE,
    );
    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 50,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let withdrawal = original.record_burn(SETTLEMENT, request).unwrap();
    original.authorize_release(RELEASE, withdrawal).unwrap();
    original
        .record_release_submission(RELEASE, withdrawal, release_reference(0))
        .unwrap();
    original
        .complete_withdrawal(FINALITY, withdrawal, finality(40, 200))
        .unwrap();

    let snapshot = original.snapshot();
    let mut restored = ExternalAssetCoordinator::from_snapshot(snapshot.clone()).unwrap();
    assert_eq!(restored.snapshot(), snapshot);
    assert_eq!(restored.audit(ASSET), original.audit(ASSET));
    assert_eq!(
        restored.deposit_record(deposit),
        original.deposit_record(deposit)
    );
    assert_eq!(
        restored.withdrawal_record(withdrawal),
        original.withdrawal_record(withdrawal)
    );
    assert!(restored.events().is_empty());
    assert_eq!(
        restored.record_mint(SETTLEMENT, deposit, MINT_REFERENCE),
        Ok(())
    );
    assert_eq!(restored.record_burn(SETTLEMENT, request), Ok(withdrawal));
}

#[test]
fn snapshot_rejects_order_and_lifecycle_corruption() {
    let mut original = coordinator();
    mint_deposit(
        &mut original,
        claim(ORIGIN_TRANSACTION, 0, 100),
        MINT_REFERENCE,
    );
    original
        .register_asset(REGISTRY, config(SECOND_ASSET, OriginAssetId::new([21; 32])))
        .unwrap();
    let snapshot = original.snapshot();

    let mut unordered = snapshot.clone();
    unordered.assets.reverse();
    assert_eq!(
        ExternalAssetCoordinator::from_snapshot(unordered).unwrap_err(),
        ExternalAssetError::NonCanonicalSnapshot
    );

    let mut invalid = snapshot;
    invalid.deposits[0].finality = None;
    assert_eq!(
        ExternalAssetCoordinator::from_snapshot(invalid).unwrap_err(),
        ExternalAssetError::SnapshotInvariant
    );
}

#[test]
fn state_codec_round_trips_and_rejects_corruption() {
    let mut original = coordinator();
    mint_deposit(
        &mut original,
        claim(ORIGIN_TRANSACTION, 0, 100),
        MINT_REFERENCE,
    );
    let snapshot = original.snapshot();
    let encoded = ExternalAssetStateCodec::encode(&snapshot).unwrap();
    assert_eq!(ExternalAssetStateCodec::decode(&encoded), Ok(snapshot));

    for truncated in 0..encoded.len() {
        assert!(ExternalAssetStateCodec::decode(&encoded[..truncated]).is_err());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert_eq!(
        ExternalAssetStateCodec::decode(&trailing),
        Err(ExternalAssetStateCodecError::TrailingBytes)
    );
    let mut wrong_domain = encoded;
    wrong_domain[0] ^= 1;
    assert_eq!(
        ExternalAssetStateCodec::decode(&wrong_domain),
        Err(ExternalAssetStateCodecError::InvalidDomain)
    );
}
