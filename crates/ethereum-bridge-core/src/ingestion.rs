use external_asset_core::FinalityEvidence;

use crate::{
    EvmAssetBinding, EvmBlockObservationSource, EvmDepositError, EvmFinalitySource,
    EvmObserverCheckpoint, EvmObserverError, EvmObserverScope, FinalizedEvmObserverBlock,
    PreparedEvmObserverBlock, VerifiedEvmDeposit, decode_deposit, prepare_observer_block,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvmObserverIngestionError<ObservationError, FinalityError> {
    InvalidBindingScope,
    DuplicateOriginAsset,
    DuplicatePayrailAsset,
    Observation(ObservationError),
    ObservationUnavailable,
    Finality(FinalityError),
    BlockNotFinalized,
    FinalizedBlockMismatch,
    Deposit(EvmDepositError),
    UnknownAsset,
    Observer(EvmObserverError),
}

/// Reads, independently finalizes, decodes and prepares the next observer
/// block. This function does not persist or mint anything; its result must be
/// committed by the durable observer journal before downstream settlement.
///
/// # Errors
///
/// Returns a typed configuration, observation, finality, deposit or observer
/// error. No prepared block is returned from partial or conflicting evidence.
pub fn ingest_next_observer_block<Observation, Finality>(
    scope: EvmObserverScope,
    checkpoint: EvmObserverCheckpoint,
    bindings: &[EvmAssetBinding],
    observation: &Observation,
    finality: &Finality,
) -> Result<PreparedEvmObserverBlock, EvmObserverIngestionError<Observation::Error, Finality::Error>>
where
    Observation: EvmBlockObservationSource,
    Finality: EvmFinalitySource,
{
    validate_bindings(scope, bindings)?;
    let observed = observation
        .observed_block(scope.chain_id, scope.bridge_contract, checkpoint.next_block)
        .map_err(EvmObserverIngestionError::Observation)?
        .ok_or(EvmObserverIngestionError::ObservationUnavailable)?;
    let finalized = finality
        .finalized_canonical_block(scope.chain_id, checkpoint.next_block)
        .map_err(EvmObserverIngestionError::Finality)?
        .ok_or(EvmObserverIngestionError::BlockNotFinalized)?;
    if observed.number != checkpoint.next_block
        || finalized.number != checkpoint.next_block
        || observed.hash != finalized.hash
    {
        return Err(EvmObserverIngestionError::FinalizedBlockMismatch);
    }

    let mut deposits = Vec::with_capacity(observed.logs.len());
    for log in &observed.logs {
        let mut matched = None;
        for binding in bindings {
            match decode_deposit(*binding, log) {
                Ok(deposit) => {
                    matched = Some(deposit);
                    break;
                }
                Err(EvmDepositError::WrongOriginAsset) => {}
                Err(error) => return Err(EvmObserverIngestionError::Deposit(error)),
            }
        }
        let deposit = matched.ok_or(EvmObserverIngestionError::UnknownAsset)?;
        if deposit.block_number() != finalized.number || deposit.block_hash() != finalized.hash {
            return Err(EvmObserverIngestionError::FinalizedBlockMismatch);
        }
        deposits.push(VerifiedEvmDeposit {
            claim: deposit.claim(),
            evidence: FinalityEvidence {
                block_height: finalized.number,
                block_hash: *finalized.hash.as_bytes(),
                proof_hash: finalized.proof_hash,
            },
        });
    }

    prepare_observer_block(
        scope,
        checkpoint,
        FinalizedEvmObserverBlock {
            finalized,
            parent_hash: observed.parent_hash,
            deposits,
        },
    )
    .map_err(EvmObserverIngestionError::Observer)
}

fn validate_bindings<ObservationError, FinalityError>(
    scope: EvmObserverScope,
    bindings: &[EvmAssetBinding],
) -> Result<(), EvmObserverIngestionError<ObservationError, FinalityError>> {
    for (index, binding) in bindings.iter().enumerate() {
        if binding.chain_id() != scope.chain_id
            || binding.bridge_contract() != scope.bridge_contract
        {
            return Err(EvmObserverIngestionError::InvalidBindingScope);
        }
        for previous in &bindings[..index] {
            if previous.origin_asset() == binding.origin_asset() {
                return Err(EvmObserverIngestionError::DuplicateOriginAsset);
            }
            if previous.payrail_asset() == binding.payrail_asset() {
                return Err(EvmObserverIngestionError::DuplicatePayrailAsset);
            }
        }
    }
    Ok(())
}
