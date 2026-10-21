use ed25519_dalek::{Signer, SigningKey};
use external_asset_core::{
    ConnectorStatus, DestinationCommitment, ExternalAssetConfig, ExternalAssetCoordinator,
    FinalityEvidence, OriginAssetId, OriginDeposit, OriginNetworkId, OriginReleaseReference,
    OriginTransactionId, SettlementReference, WithdrawalRequest,
};
use external_asset_settlement_core::{
    AuthorizedWithdrawal, ExternalAssetSettlement, ExternalSettlementError, SettlementOutcome,
    SignedWithdrawalRequest,
};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, BackingRequirement, Ledger,
    LedgerError, NetworkId, SignatureBytes, SignatureVerifier,
};
use transaction_auth_ed25519::Ed25519Verifier;

const REGISTRY: AccountId = AccountId::new([1; 32]);
const OBSERVER: AccountId = AccountId::new([2; 32]);
const FINALITY: AccountId = AccountId::new([3; 32]);
const SETTLEMENT: AccountId = AccountId::new([4; 32]);
const RELEASE: AccountId = AccountId::new([5; 32]);
const FREEZE: AccountId = AccountId::new([6; 32]);
const TREASURY: AccountId = AccountId::new([7; 32]);
const ALICE: AccountId = AccountId::new([8; 32]);
const ASSET: AssetId = AssetId::new([9; 32]);
const ORIGIN_NETWORK: OriginNetworkId = OriginNetworkId::new([10; 32]);
const ORIGIN_ASSET: OriginAssetId = OriginAssetId::new([11; 32]);
const TRANSACTION: OriginTransactionId = OriginTransactionId::new([12; 32]);
const MINT_REFERENCE: SettlementReference = SettlementReference::new([13; 32]);
const BURN_REFERENCE: SettlementReference = SettlementReference::new([14; 32]);
const DESTINATION: DestinationCommitment = DestinationCommitment::new([15; 32]);
const NETWORK: NetworkId = NetworkId::new([21; 32]);
const WITHDRAWAL_SIGNATURE: SignatureBytes = SignatureBytes::new([42; 64]);

struct ExactVerifier {
    signer: AccountId,
    message: Vec<u8>,
    signature: SignatureBytes,
}

impl SignatureVerifier for ExactVerifier {
    fn verify(&self, signer: AccountId, message: &[u8], signature: SignatureBytes) -> bool {
        signer == self.signer && message == self.message && signature == self.signature
    }
}

fn definition(max_supply: Option<u128>) -> AssetDefinition {
    AssetDefinition {
        id: ASSET,
        symbol: "WETH".to_owned(),
        decimals: 18,
        class: AssetClass::ExternalRepresentation,
        status: AssetStatus::Test,
        issuer: SETTLEMENT,
        backing_authority: FINALITY,
        freeze_authority: FREEZE,
        treasury: TREASURY,
        max_supply,
        backing_requirement: BackingRequirement::VerifiedOneToOne,
    }
}

fn config() -> ExternalAssetConfig {
    ExternalAssetConfig {
        asset: ASSET,
        origin_network: ORIGIN_NETWORK,
        origin_asset: ORIGIN_ASSET,
        observer_authority: OBSERVER,
        finality_authority: FINALITY,
        settlement_authority: SETTLEMENT,
        release_authority: RELEASE,
        status: ConnectorStatus::Active,
    }
}

fn settlement(max_supply: Option<u128>) -> ExternalAssetSettlement {
    let ledger = Ledger::new(NETWORK, REGISTRY);
    let external = ExternalAssetCoordinator::new(REGISTRY);
    let mut combined = ExternalAssetSettlement::new(ledger, external);
    combined
        .register_asset(REGISTRY, definition(max_supply), config())
        .unwrap();
    combined
}

fn authorize_withdrawal(
    request: WithdrawalRequest,
    current_height: u64,
    valid_until_height: u64,
) -> AuthorizedWithdrawal {
    let signed = SignedWithdrawalRequest {
        network: NETWORK,
        request,
        valid_until_height,
        signature: WITHDRAWAL_SIGNATURE,
    };
    let verifier = ExactVerifier {
        signer: request.account,
        message: signed.authorization_message(),
        signature: WITHDRAWAL_SIGNATURE,
    };
    signed.verify(NETWORK, current_height, &verifier).unwrap()
}

fn claim(amount: u128) -> OriginDeposit {
    OriginDeposit {
        asset: ASSET,
        origin_network: ORIGIN_NETWORK,
        origin_asset: ORIGIN_ASSET,
        transaction: TRANSACTION,
        event_index: 1,
        recipient: ALICE,
        amount,
    }
}

fn evidence(byte: u8, height: u64) -> FinalityEvidence {
    FinalityEvidence {
        block_height: height,
        block_hash: [byte; 32],
        proof_hash: [byte.wrapping_add(1); 32],
    }
}

fn settle(
    state: &mut ExternalAssetSettlement,
    amount: u128,
) -> SettlementOutcome<external_asset_core::DepositId> {
    state
        .settle_deposit(
            OBSERVER,
            FINALITY,
            SETTLEMENT,
            claim(amount),
            evidence(30, 100),
            MINT_REFERENCE,
        )
        .unwrap()
}

#[test]
fn finalized_deposit_updates_backing_mints_and_retries_once() {
    let mut state = settlement(None);
    let first = settle(&mut state, 1_000);
    let id = match first {
        SettlementOutcome::Applied(id) => id,
        SettlementOutcome::ExistingSame(_) => panic!("first transition must apply"),
    };
    assert_eq!(state.ledger().balance(ASSET, ALICE), 1_000);
    assert_eq!(state.ledger().total_supply(ASSET), Some(1_000));
    assert_eq!(
        state.ledger().audit_asset(ASSET).unwrap().verified_backing,
        1_000
    );
    assert_eq!(
        state.external().audit(ASSET).unwrap().represented_supply,
        1_000
    );

    assert_eq!(
        settle(&mut state, 1_000),
        SettlementOutcome::ExistingSame(id)
    );
    assert_eq!(state.ledger().balance(ASSET, ALICE), 1_000);
}

#[test]
fn late_ledger_failure_rolls_back_external_finality_and_backing() {
    let mut state = settlement(Some(50));
    assert_eq!(
        state.settle_deposit(
            OBSERVER,
            FINALITY,
            SETTLEMENT,
            claim(100),
            evidence(30, 100),
            MINT_REFERENCE,
        ),
        Err(ExternalSettlementError::Ledger(
            LedgerError::SupplyCapExceeded
        ))
    );
    assert!(
        state
            .external()
            .deposit_record(external_asset_core::DepositId::derive(&claim(100)))
            .is_none()
    );
    assert_eq!(
        state.ledger().audit_asset(ASSET).unwrap().verified_backing,
        0
    );
    assert_eq!(state.ledger().total_supply(ASSET), Some(0));
}

#[test]
fn burn_release_and_finality_keep_supply_and_backing_reconciled() {
    let mut state = settlement(None);
    settle(&mut state, 1_000);
    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 300,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let authorized = authorize_withdrawal(request, 100, 110);
    let withdrawal = match state.burn_withdrawal(SETTLEMENT, 100, authorized).unwrap() {
        SettlementOutcome::Applied(id) => id,
        SettlementOutcome::ExistingSame(_) => panic!("first burn must apply"),
    };
    assert_eq!(state.ledger().balance(ASSET, ALICE), 700);
    assert_eq!(
        state.burn_withdrawal(SETTLEMENT, 100, authorized),
        Ok(SettlementOutcome::ExistingSame(withdrawal))
    );
    let release = state.authorize_release(RELEASE, withdrawal).unwrap();
    assert_eq!(release.amount, 300);
    state
        .record_release_submission(
            RELEASE,
            withdrawal,
            OriginReleaseReference {
                transaction: OriginTransactionId::new([16; 32]),
                event_index: 2,
            },
        )
        .unwrap();
    state
        .complete_withdrawal(FINALITY, withdrawal, evidence(40, 200))
        .unwrap();

    let ledger = state.ledger().audit_asset(ASSET).unwrap();
    let external = state.external().audit(ASSET).unwrap();
    assert_eq!(ledger.declared_supply, 700);
    assert_eq!(ledger.verified_backing, 700);
    assert_eq!(external.represented_supply, 700);
    assert_eq!(external.expected_backing, 700);
}

#[test]
fn withdrawal_requires_network_bound_owner_signature_and_live_expiry() {
    let mut state = settlement(None);
    settle(&mut state, 1_000);
    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 300,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let signed = SignedWithdrawalRequest {
        network: NETWORK,
        request,
        valid_until_height: 110,
        signature: WITHDRAWAL_SIGNATURE,
    };
    let verifier = ExactVerifier {
        signer: ALICE,
        message: signed.authorization_message(),
        signature: WITHDRAWAL_SIGNATURE,
    };

    let mut changed = signed;
    changed.request.amount = 301;
    assert_eq!(
        changed.verify(NETWORK, 100, &verifier),
        Err(ExternalSettlementError::InvalidWithdrawalSignature)
    );
    assert_eq!(
        signed.verify(NetworkId::new([99; 32]), 100, &verifier),
        Err(ExternalSettlementError::WrongWithdrawalNetwork)
    );
    assert_eq!(
        signed.verify(NETWORK, 111, &verifier),
        Err(ExternalSettlementError::ExpiredWithdrawalAuthorization)
    );

    let authorized = signed.verify(NETWORK, 100, &verifier).unwrap();
    assert_eq!(
        state.burn_withdrawal(SETTLEMENT, 111, authorized),
        Err(ExternalSettlementError::ExpiredWithdrawalAuthorization)
    );
    assert_eq!(state.ledger().balance(ASSET, ALICE), 1_000);
    assert!(
        state
            .external()
            .withdrawal_record(external_asset_core::WithdrawalId::derive(&request))
            .is_none()
    );
}

#[test]
fn withdrawal_signature_covers_every_monetary_and_routing_field() {
    let request = WithdrawalRequest {
        asset: ASSET,
        account: ALICE,
        amount: 300,
        destination: DESTINATION,
        burn_reference: BURN_REFERENCE,
    };
    let signed = SignedWithdrawalRequest {
        network: NETWORK,
        request,
        valid_until_height: 110,
        signature: WITHDRAWAL_SIGNATURE,
    };
    let verifier = ExactVerifier {
        signer: ALICE,
        message: signed.authorization_message(),
        signature: WITHDRAWAL_SIGNATURE,
    };
    let mut mutations = Vec::new();

    let mut changed = signed;
    changed.network = NetworkId::new([99; 32]);
    mutations.push(changed);
    let mut changed = signed;
    changed.request.asset = AssetId::new([99; 32]);
    mutations.push(changed);
    let mut changed = signed;
    changed.request.account = AccountId::new([99; 32]);
    mutations.push(changed);
    let mut changed = signed;
    changed.request.amount += 1;
    mutations.push(changed);
    let mut changed = signed;
    changed.request.destination = DestinationCommitment::new([99; 32]);
    mutations.push(changed);
    let mut changed = signed;
    changed.request.burn_reference = SettlementReference::new([99; 32]);
    mutations.push(changed);
    let mut changed = signed;
    changed.valid_until_height += 1;
    mutations.push(changed);
    let mut changed = signed;
    changed.signature = SignatureBytes::new([99; 64]);
    mutations.push(changed);

    for changed in mutations {
        let expected = if changed.network == NETWORK {
            ExternalSettlementError::InvalidWithdrawalSignature
        } else {
            ExternalSettlementError::WrongWithdrawalNetwork
        };
        assert_eq!(changed.verify(NETWORK, 100, &verifier), Err(expected));
    }
}

#[test]
fn strict_ed25519_verifier_accepts_the_canonical_withdrawal_message() {
    let key = SigningKey::from_bytes(&[77; 32]);
    let mut signed = SignedWithdrawalRequest {
        network: NETWORK,
        request: WithdrawalRequest {
            asset: ASSET,
            account: AccountId::new(key.verifying_key().to_bytes()),
            amount: 300,
            destination: DESTINATION,
            burn_reference: BURN_REFERENCE,
        },
        valid_until_height: 110,
        signature: SignatureBytes::new([0; 64]),
    };
    signed.signature = SignatureBytes::new(key.sign(&signed.authorization_message()).to_bytes());

    assert!(signed.verify(NETWORK, 100, &Ed25519Verifier).is_ok());
    signed.request.amount += 1;
    assert_eq!(
        signed.verify(NETWORK, 100, &Ed25519Verifier),
        Err(ExternalSettlementError::InvalidWithdrawalSignature)
    );
}

#[test]
fn wrong_role_and_policy_mismatch_fail_without_registration() {
    let ledger = Ledger::new(NETWORK, REGISTRY);
    let external = ExternalAssetCoordinator::new(REGISTRY);
    let mut state = ExternalAssetSettlement::new(ledger, external);
    let mut wrong = definition(None);
    wrong.issuer = OBSERVER;
    assert_eq!(
        state.register_asset(REGISTRY, wrong, config()),
        Err(ExternalSettlementError::AssetDefinitionMismatch)
    );
    assert!(state.ledger().definition(ASSET).is_none());

    let mut state = settlement(None);
    assert_eq!(
        state.settle_deposit(
            SETTLEMENT,
            FINALITY,
            SETTLEMENT,
            claim(1),
            evidence(1, 1),
            MINT_REFERENCE,
        ),
        Err(ExternalSettlementError::External(
            external_asset_core::ExternalAssetError::Unauthorized
        ))
    );
    assert_eq!(state.ledger().total_supply(ASSET), Some(0));
}

#[test]
fn combined_snapshot_restart_preserves_reconciliation_and_replay_guards() {
    let mut state = settlement(None);
    let first = settle(&mut state, 500);
    let snapshot = state.snapshot();
    let mut restored =
        ExternalAssetSettlement::from_snapshot(NetworkId::new([21; 32]), snapshot.clone()).unwrap();
    assert_eq!(restored.snapshot(), snapshot);
    let expected_id = match first {
        SettlementOutcome::Applied(id) => id,
        SettlementOutcome::ExistingSame(_) => panic!("first transition must apply"),
    };
    assert_eq!(
        settle(&mut restored, 500),
        SettlementOutcome::ExistingSame(expected_id)
    );

    let mut corrupt = snapshot;
    corrupt.ledger.assets[0].supply = 499;
    assert!(ExternalAssetSettlement::from_snapshot(NetworkId::new([21; 32]), corrupt).is_err());
}
