use external_asset_core::{
    DepositId, ExternalAssetConfig, ExternalAssetCoordinator, ReleaseInstruction, WithdrawalId,
};
use external_asset_settlement_core::ExternalAssetSettlement;
use ledger_core::{
    AccountId, AssetDefinition, Event, Ledger, NetworkId, OperationReceipt, SignatureVerifier,
    SignedOperation,
};
use ledger_runtime_core::{LedgerBlockCodec, LedgerStateCodec, block_hash, state_root};
use state_sync_core::FinalizedCheckpoint;
use tail_sync_core::{PreparedTailTransition, TailCommitment, TailTransitionExecutor};

use crate::{
    BridgeAuthorizationRole, CompleteWithdrawalOperation, DepositSettlementOperation,
    ExternalSettlementBlockCodec, ExternalSettlementOperation, ExternalSettlementRuntimeError,
    ExternalSettlementStateCodec, WithdrawalSettlementOperation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalSettlementRuntimeConfig {
    pub network: NetworkId,
    pub activation_height: u64,
    pub registry_authority: AccountId,
    pub asset_definition: AssetDefinition,
    pub external_asset: ExternalAssetConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalSettlementExecutionKind {
    Ledger(Vec<SignedOperation>),
    Deposit(DepositId),
    Withdrawal {
        withdrawal: WithdrawalId,
        release: ReleaseInstruction,
    },
    CompleteWithdrawal(WithdrawalId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutedExternalSettlementBlock {
    pub transition: PreparedTailTransition,
    pub receipts: Vec<OperationReceipt>,
    pub events: Vec<Event>,
    pub kind: ExternalSettlementExecutionKind,
}

#[derive(Clone, Debug)]
pub struct ExternalSettlementBlockExecutor<V> {
    config: ExternalSettlementRuntimeConfig,
    verifier: V,
}

impl<V> ExternalSettlementBlockExecutor<V> {
    #[must_use]
    pub const fn new(config: ExternalSettlementRuntimeConfig, verifier: V) -> Self {
        Self { config, verifier }
    }

    #[must_use]
    pub const fn config(&self) -> &ExternalSettlementRuntimeConfig {
        &self.config
    }

    /// Restores combined state, migrating a legacy ledger image in memory.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt state or an invalid configured asset.
    pub fn settlement_state(
        &self,
        state: &[u8],
    ) -> Result<ExternalAssetSettlement, ExternalSettlementRuntimeError> {
        match ExternalSettlementStateCodec::decode(self.config.network, state) {
            Ok(snapshot) => ExternalAssetSettlement::from_snapshot(self.config.network, snapshot)
                .map_err(Into::into),
            Err(_) => self.migrate_legacy_state(state),
        }
    }

    fn migrate_legacy_state(
        &self,
        state: &[u8],
    ) -> Result<ExternalAssetSettlement, ExternalSettlementRuntimeError> {
        let snapshot = LedgerStateCodec::decode(self.config.network, state)?;
        let ledger = Ledger::from_snapshot(self.config.network, snapshot)
            .map_err(ExternalSettlementRuntimeError::LegacyState)?;
        let external = ExternalAssetCoordinator::new(self.config.registry_authority);
        let mut settlement = ExternalAssetSettlement::new(ledger, external);
        settlement.register_asset(
            self.config.registry_authority,
            self.config.asset_definition.clone(),
            self.config.external_asset,
        )?;
        Ok(settlement)
    }
}

impl<V: SignatureVerifier> ExternalSettlementBlockExecutor<V> {
    /// Executes either a legacy ledger block or one bridge authority operation.
    /// Every transition produces the combined canonical state format.
    ///
    /// # Errors
    ///
    /// Returns an error before publication for state, signature, role, replay,
    /// monetary or codec failures.
    pub fn execute(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
    ) -> Result<ExecutedExternalSettlementBlock, ExternalSettlementRuntimeError> {
        if state_root(previous_state) != previous.state_root {
            return Err(ExternalSettlementRuntimeError::PreviousStateRootMismatch);
        }
        let next_height = previous
            .height
            .checked_add(1)
            .ok_or(ExternalSettlementRuntimeError::InvalidStateHeight)?;
        if next_height < self.config.activation_height {
            return self.execute_legacy(previous, previous_state, payload, next_height);
        }
        let mut settlement = self.settlement_state(previous_state)?;
        let (kind, receipts) = if ExternalSettlementBlockCodec::is_external(payload) {
            let operation = ExternalSettlementBlockCodec::decode(payload)?;
            (
                self.execute_external(&mut settlement, next_height, &operation)?,
                Vec::new(),
            )
        } else {
            self.execute_ledger(&mut settlement, next_height, payload)?
        };
        let events = settlement.ledger().events().to_vec();
        let state = ExternalSettlementStateCodec::encode(&settlement.snapshot())?;
        Ok(ExecutedExternalSettlementBlock {
            transition: PreparedTailTransition {
                commitment: TailCommitment {
                    block_hash: block_hash(previous.block_hash, payload),
                    state_root: state_root(&state),
                },
                state,
            },
            receipts,
            events,
            kind,
        })
    }

    fn execute_legacy(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
        next_height: u64,
    ) -> Result<ExecutedExternalSettlementBlock, ExternalSettlementRuntimeError> {
        if ExternalSettlementBlockCodec::is_external(payload) {
            return Err(ExternalSettlementRuntimeError::InvalidOperation);
        }
        let snapshot = LedgerStateCodec::decode(self.config.network, previous_state)?;
        let mut ledger = Ledger::from_snapshot(self.config.network, snapshot)
            .map_err(ExternalSettlementRuntimeError::LegacyState)?;
        let operations = LedgerBlockCodec::decode(payload)?;
        let mut receipts = Vec::with_capacity(operations.len());
        for operation in operations.iter().cloned() {
            let verified = operation
                .verify_for_network(self.config.network, &self.verifier)
                .map_err(ExternalSettlementRuntimeError::Ledger)?;
            receipts.push(
                ledger
                    .submit_verified_at_height(verified, next_height)
                    .map_err(ExternalSettlementRuntimeError::Ledger)?,
            );
        }
        let events = ledger.events().to_vec();
        let state = LedgerStateCodec::encode(&ledger.snapshot())?;
        Ok(ExecutedExternalSettlementBlock {
            transition: PreparedTailTransition {
                commitment: TailCommitment {
                    block_hash: block_hash(previous.block_hash, payload),
                    state_root: state_root(&state),
                },
                state,
            },
            receipts,
            events,
            kind: ExternalSettlementExecutionKind::Ledger(operations),
        })
    }

    fn execute_ledger(
        &self,
        settlement: &mut ExternalAssetSettlement,
        next_height: u64,
        payload: &[u8],
    ) -> Result<
        (ExternalSettlementExecutionKind, Vec<OperationReceipt>),
        ExternalSettlementRuntimeError,
    > {
        let operations = LedgerBlockCodec::decode(payload)?;
        let (mut ledger, external) = settlement.clone().into_parts();
        let mut receipts = Vec::with_capacity(operations.len());
        for operation in operations.iter().cloned() {
            let verified = operation
                .verify_for_network(self.config.network, &self.verifier)
                .map_err(ExternalSettlementRuntimeError::Ledger)?;
            receipts.push(
                ledger
                    .submit_verified_at_height(verified, next_height)
                    .map_err(ExternalSettlementRuntimeError::Ledger)?,
            );
        }
        *settlement = ExternalAssetSettlement::new(ledger, external);
        Ok((
            ExternalSettlementExecutionKind::Ledger(operations),
            receipts,
        ))
    }

    fn execute_external(
        &self,
        settlement: &mut ExternalAssetSettlement,
        next_height: u64,
        operation: &ExternalSettlementOperation,
    ) -> Result<ExternalSettlementExecutionKind, ExternalSettlementRuntimeError> {
        match operation {
            ExternalSettlementOperation::Deposit(operation) => {
                self.execute_deposit(settlement, operation)
            }
            ExternalSettlementOperation::Withdrawal(operation) => {
                self.execute_withdrawal(settlement, next_height, operation)
            }
            ExternalSettlementOperation::CompleteWithdrawal(operation) => {
                self.execute_completion(settlement, operation)
            }
        }
    }

    fn execute_deposit(
        &self,
        settlement: &mut ExternalAssetSettlement,
        operation: &DepositSettlementOperation,
    ) -> Result<ExternalSettlementExecutionKind, ExternalSettlementRuntimeError> {
        self.require_network_asset(operation.network, operation.claim.asset)?;
        let config = self.config.external_asset;
        verify_authorization(
            &self.verifier,
            operation.observer_authorization,
            config.observer_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Observer),
        )?;
        verify_authorization(
            &self.verifier,
            operation.finality_authorization,
            config.finality_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Finality),
        )?;
        verify_authorization(
            &self.verifier,
            operation.settlement_authorization,
            config.settlement_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Settlement),
        )?;
        let outcome = settlement.settle_deposit(
            config.observer_authority,
            config.finality_authority,
            config.settlement_authority,
            operation.claim,
            operation.evidence,
            operation.reference,
        )?;
        let deposit = match outcome {
            external_asset_settlement_core::SettlementOutcome::Applied(id)
            | external_asset_settlement_core::SettlementOutcome::ExistingSame(id) => id,
        };
        Ok(ExternalSettlementExecutionKind::Deposit(deposit))
    }

    fn execute_withdrawal(
        &self,
        settlement: &mut ExternalAssetSettlement,
        next_height: u64,
        operation: &WithdrawalSettlementOperation,
    ) -> Result<ExternalSettlementExecutionKind, ExternalSettlementRuntimeError> {
        self.require_network_asset(operation.request.network, operation.request.request.asset)?;
        let config = self.config.external_asset;
        verify_authorization(
            &self.verifier,
            operation.settlement_authorization,
            config.settlement_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Settlement),
        )?;
        verify_authorization(
            &self.verifier,
            operation.release_authorization,
            config.release_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Release),
        )?;
        let authorized =
            operation
                .request
                .verify(self.config.network, next_height, &self.verifier)?;
        let outcome =
            settlement.burn_withdrawal(config.settlement_authority, next_height, authorized)?;
        let withdrawal = match outcome {
            external_asset_settlement_core::SettlementOutcome::Applied(id)
            | external_asset_settlement_core::SettlementOutcome::ExistingSame(id) => id,
        };
        let release = settlement.authorize_release(config.release_authority, withdrawal)?;
        Ok(ExternalSettlementExecutionKind::Withdrawal {
            withdrawal,
            release,
        })
    }

    fn execute_completion(
        &self,
        settlement: &mut ExternalAssetSettlement,
        operation: &CompleteWithdrawalOperation,
    ) -> Result<ExternalSettlementExecutionKind, ExternalSettlementRuntimeError> {
        if operation.network != self.config.network {
            return Err(ExternalSettlementRuntimeError::WrongNetwork);
        }
        let config = self.config.external_asset;
        verify_authorization(
            &self.verifier,
            operation.release_authorization,
            config.release_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Release),
        )?;
        verify_authorization(
            &self.verifier,
            operation.finality_authorization,
            config.finality_authority,
            &operation.authorization_message(BridgeAuthorizationRole::Finality),
        )?;
        settlement.record_release_submission(
            config.release_authority,
            operation.withdrawal,
            operation.release_reference,
        )?;
        settlement.complete_withdrawal(
            config.finality_authority,
            operation.withdrawal,
            operation.evidence,
        )?;
        Ok(ExternalSettlementExecutionKind::CompleteWithdrawal(
            operation.withdrawal,
        ))
    }

    fn require_network_asset(
        &self,
        network: NetworkId,
        asset: ledger_core::AssetId,
    ) -> Result<(), ExternalSettlementRuntimeError> {
        if network != self.config.network {
            return Err(ExternalSettlementRuntimeError::WrongNetwork);
        }
        if asset != self.config.asset_definition.id || asset != self.config.external_asset.asset {
            return Err(ExternalSettlementRuntimeError::WrongAsset);
        }
        Ok(())
    }
}

impl<V: SignatureVerifier> TailTransitionExecutor for ExternalSettlementBlockExecutor<V> {
    fn prepare_transition(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
    ) -> Option<PreparedTailTransition> {
        self.execute(previous, previous_state, payload)
            .ok()
            .map(|executed| executed.transition)
    }
}

fn verify_authorization<V: SignatureVerifier>(
    verifier: &V,
    authorization: ledger_core::Authorization,
    expected: AccountId,
    message: &[u8],
) -> Result<(), ExternalSettlementRuntimeError> {
    if authorization.signer != expected {
        return Err(ExternalSettlementRuntimeError::WrongAuthority);
    }
    if !verifier.verify(authorization.signer, message, authorization.signature) {
        return Err(ExternalSettlementRuntimeError::InvalidAuthoritySignature);
    }
    Ok(())
}
