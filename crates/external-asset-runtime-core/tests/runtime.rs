use ed25519_dalek::{Signer, SigningKey};
use external_asset_core::{
    ConnectorStatus, DepositId, DestinationCommitment, ExternalAssetConfig, FinalityEvidence,
    OriginAssetId, OriginDeposit, OriginNetworkId, OriginReleaseReference, OriginTransactionId,
    SettlementReference, WithdrawalRequest,
};
use external_asset_runtime_core::{
    BridgeAuthorizationRole, CompleteWithdrawalOperation, DepositSettlementOperation,
    ExternalSettlementBlockCodec, ExternalSettlementBlockExecutor, ExternalSettlementExecutionKind,
    ExternalSettlementOperation, ExternalSettlementRuntimeConfig, WithdrawalSettlementOperation,
};
use external_asset_settlement_core::SignedWithdrawalRequest;
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization,
    BackingRequirement, Ledger, NetworkId, SignatureBytes,
};
use ledger_runtime_core::{LedgerStateCodec, state_root};
use state_sync_core::{BlockHash, FinalizedCheckpoint, ValidatorSetHash};
use transaction_auth_ed25519::Ed25519Verifier;

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const REGISTRY: AccountId = AccountId::new([2; 32]);
const ASSET: AssetId = AssetId::new([3; 32]);
const ORIGIN_NETWORK: OriginNetworkId = OriginNetworkId::new([4; 32]);
const ORIGIN_ASSET: OriginAssetId = OriginAssetId::new([5; 32]);
const DESTINATION: DestinationCommitment = DestinationCommitment::new([6; 32]);

fn key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn account(key: &SigningKey) -> AccountId {
    AccountId::new(key.verifying_key().to_bytes())
}

fn authorization(key: &SigningKey, message: &[u8]) -> Authorization {
    Authorization {
        signer: account(key),
        signature: SignatureBytes::new(key.sign(message).to_bytes()),
    }
}

struct Fixture {
    observer: SigningKey,
    finality: SigningKey,
    settlement: SigningKey,
    release: SigningKey,
    user: SigningKey,
    executor: ExternalSettlementBlockExecutor<Ed25519Verifier>,
    checkpoint: FinalizedCheckpoint,
    state: Vec<u8>,
}

impl Fixture {
    fn new(max_supply: u128) -> Self {
        let observer = key(11);
        let finality = key(12);
        let settlement = key(13);
        let release = key(14);
        let user = key(15);
        let ledger = Ledger::new(NETWORK, REGISTRY);
        let state = LedgerStateCodec::encode(&ledger.snapshot()).unwrap();
        let checkpoint = FinalizedCheckpoint {
            height: 0,
            block_hash: BlockHash::new([16; 32]),
            state_root: state_root(&state),
            validator_set_hash: ValidatorSetHash::new([17; 32]),
        };
        let executor = ExternalSettlementBlockExecutor::new(
            ExternalSettlementRuntimeConfig {
                network: NETWORK,
                activation_height: 1,
                registry_authority: REGISTRY,
                asset_definition: AssetDefinition {
                    id: ASSET,
                    symbol: "TUSDT".to_owned(),
                    decimals: 6,
                    class: AssetClass::ExternalRepresentation,
                    status: AssetStatus::Test,
                    issuer: account(&settlement),
                    backing_authority: account(&finality),
                    freeze_authority: REGISTRY,
                    treasury: REGISTRY,
                    max_supply: Some(max_supply),
                    backing_requirement: BackingRequirement::VerifiedOneToOne,
                },
                external_asset: ExternalAssetConfig {
                    asset: ASSET,
                    origin_network: ORIGIN_NETWORK,
                    origin_asset: ORIGIN_ASSET,
                    observer_authority: account(&observer),
                    finality_authority: account(&finality),
                    settlement_authority: account(&settlement),
                    release_authority: account(&release),
                    status: ConnectorStatus::Active,
                },
            },
            Ed25519Verifier,
        );
        Self {
            observer,
            finality,
            settlement,
            release,
            user,
            executor,
            checkpoint,
            state,
        }
    }

    fn claim(&self, amount: u128) -> OriginDeposit {
        OriginDeposit {
            asset: ASSET,
            origin_network: ORIGIN_NETWORK,
            origin_asset: ORIGIN_ASSET,
            transaction: OriginTransactionId::new([18; 32]),
            event_index: 1,
            recipient: account(&self.user),
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

    fn deposit(&self, amount: u128) -> DepositSettlementOperation {
        let blank = Authorization {
            signer: AccountId::new([0; 32]),
            signature: SignatureBytes::new([0; 64]),
        };
        let mut operation = DepositSettlementOperation {
            network: NETWORK,
            claim: self.claim(amount),
            evidence: Self::evidence(20, 100),
            reference: SettlementReference::new([21; 32]),
            observer_authorization: blank,
            finality_authorization: blank,
            settlement_authorization: blank,
        };
        operation.observer_authorization = authorization(
            &self.observer,
            &operation.authorization_message(BridgeAuthorizationRole::Observer),
        );
        operation.finality_authorization = authorization(
            &self.finality,
            &operation.authorization_message(BridgeAuthorizationRole::Finality),
        );
        operation.settlement_authorization = authorization(
            &self.settlement,
            &operation.authorization_message(BridgeAuthorizationRole::Settlement),
        );
        operation
    }

    fn withdrawal(&self, amount: u128) -> WithdrawalSettlementOperation {
        let mut request = SignedWithdrawalRequest {
            network: NETWORK,
            request: WithdrawalRequest {
                asset: ASSET,
                account: account(&self.user),
                amount,
                destination: DESTINATION,
                burn_reference: SettlementReference::new([22; 32]),
            },
            valid_until_height: 20,
            signature: SignatureBytes::new([0; 64]),
        };
        request.signature =
            SignatureBytes::new(self.user.sign(&request.authorization_message()).to_bytes());
        let blank = Authorization {
            signer: AccountId::new([0; 32]),
            signature: SignatureBytes::new([0; 64]),
        };
        let mut operation = WithdrawalSettlementOperation {
            request,
            settlement_authorization: blank,
            release_authorization: blank,
        };
        operation.settlement_authorization = authorization(
            &self.settlement,
            &operation.authorization_message(BridgeAuthorizationRole::Settlement),
        );
        operation.release_authorization = authorization(
            &self.release,
            &operation.authorization_message(BridgeAuthorizationRole::Release),
        );
        operation
    }

    fn execute(
        &mut self,
        operation: &ExternalSettlementOperation,
    ) -> ExternalSettlementExecutionKind {
        let payload = ExternalSettlementBlockCodec::encode(*operation).unwrap();
        let executed = self
            .executor
            .execute(self.checkpoint, &self.state, &payload)
            .unwrap();
        self.checkpoint = FinalizedCheckpoint {
            height: self.checkpoint.height + 1,
            block_hash: executed.transition.commitment.block_hash,
            state_root: executed.transition.commitment.state_root,
            validator_set_hash: self.checkpoint.validator_set_hash,
        };
        self.state = executed.transition.state;
        executed.kind
    }
}

#[test]
fn codec_rejects_every_truncation_and_trailing_bytes() {
    let fixture = Fixture::new(1_000);
    let encoded = ExternalSettlementBlockCodec::encode(ExternalSettlementOperation::Deposit(
        fixture.deposit(500),
    ))
    .unwrap();
    assert_eq!(
        ExternalSettlementBlockCodec::decode(&encoded),
        Ok(ExternalSettlementOperation::Deposit(fixture.deposit(500)))
    );
    for end in 0..encoded.len() {
        assert!(ExternalSettlementBlockCodec::decode(&encoded[..end]).is_err());
    }
    let mut trailing = encoded;
    trailing.push(0);
    assert!(ExternalSettlementBlockCodec::decode(&trailing).is_err());
}

#[test]
fn deposit_replay_does_not_double_mint_and_wrong_role_is_rejected() {
    let mut fixture = Fixture::new(1_000);
    let operation = fixture.deposit(500);
    assert_eq!(
        fixture.execute(&ExternalSettlementOperation::Deposit(operation)),
        ExternalSettlementExecutionKind::Deposit(DepositId::derive(&fixture.claim(500)))
    );
    assert_eq!(
        fixture.execute(&ExternalSettlementOperation::Deposit(operation)),
        ExternalSettlementExecutionKind::Deposit(DepositId::derive(&fixture.claim(500)))
    );
    let settlement = fixture.executor.settlement_state(&fixture.state).unwrap();
    assert_eq!(
        settlement.ledger().balance(ASSET, account(&fixture.user)),
        500
    );

    let mut wrong = fixture.deposit(1);
    wrong.observer_authorization.signer = account(&fixture.settlement);
    let payload =
        ExternalSettlementBlockCodec::encode(ExternalSettlementOperation::Deposit(wrong)).unwrap();
    assert!(
        fixture
            .executor
            .execute(fixture.checkpoint, &fixture.state, &payload)
            .is_err()
    );
    assert_eq!(
        settlement.ledger().balance(ASSET, account(&fixture.user)),
        500
    );
}

#[test]
fn late_mint_failure_and_invalid_signature_leave_previous_state_unchanged() {
    let fixture = Fixture::new(100);
    let previous = fixture.state.clone();
    let payload = ExternalSettlementBlockCodec::encode(ExternalSettlementOperation::Deposit(
        fixture.deposit(101),
    ))
    .unwrap();
    assert!(
        fixture
            .executor
            .execute(fixture.checkpoint, &fixture.state, &payload)
            .is_err()
    );
    assert_eq!(fixture.state, previous);

    let mut changed = fixture.deposit(50);
    changed.claim.amount = 51;
    let payload =
        ExternalSettlementBlockCodec::encode(ExternalSettlementOperation::Deposit(changed))
            .unwrap();
    assert!(
        fixture
            .executor
            .execute(fixture.checkpoint, &fixture.state, &payload)
            .is_err()
    );
    assert_eq!(fixture.state, previous);
}

#[test]
fn deposit_burn_release_and_completion_reconcile_supply() {
    let mut fixture = Fixture::new(1_000);
    fixture.execute(&ExternalSettlementOperation::Deposit(fixture.deposit(500)));
    let kind = fixture.execute(&ExternalSettlementOperation::Withdrawal(
        fixture.withdrawal(200),
    ));
    let ExternalSettlementExecutionKind::Withdrawal {
        withdrawal,
        release,
    } = kind
    else {
        panic!("expected withdrawal");
    };
    assert_eq!(release.amount, 200);
    assert_eq!(release.destination, DESTINATION);

    let blank = Authorization {
        signer: AccountId::new([0; 32]),
        signature: SignatureBytes::new([0; 64]),
    };
    let mut completion = CompleteWithdrawalOperation {
        network: NETWORK,
        withdrawal,
        release_reference: OriginReleaseReference {
            transaction: OriginTransactionId::new([23; 32]),
            event_index: 0,
        },
        evidence: Fixture::evidence(24, 200),
        release_authorization: blank,
        finality_authorization: blank,
    };
    completion.release_authorization = authorization(
        &fixture.release,
        &completion.authorization_message(BridgeAuthorizationRole::Release),
    );
    completion.finality_authorization = authorization(
        &fixture.finality,
        &completion.authorization_message(BridgeAuthorizationRole::Finality),
    );
    assert_eq!(
        fixture.execute(&ExternalSettlementOperation::CompleteWithdrawal(completion)),
        ExternalSettlementExecutionKind::CompleteWithdrawal(withdrawal)
    );
    let state = fixture.executor.settlement_state(&fixture.state).unwrap();
    assert_eq!(state.ledger().balance(ASSET, account(&fixture.user)), 300);
    assert_eq!(state.ledger().total_supply(ASSET), Some(300));
    assert_eq!(
        state.ledger().audit_asset(ASSET).unwrap().verified_backing,
        300
    );
    assert_eq!(state.external().audit(ASSET).unwrap().expected_backing, 300);
}
