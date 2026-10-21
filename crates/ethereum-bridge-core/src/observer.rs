use crate::{
    EvmAddress, EvmChainId, EvmFinalizedBlock, EvmHash, VerifiedEvmDeposit, origin_network_id,
};

pub const MAX_DEPOSITS_PER_BLOCK: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmObserverScope {
    pub chain_id: EvmChainId,
    pub bridge_contract: EvmAddress,
    pub first_block: u64,
    pub first_parent_hash: EvmHash,
}

impl EvmObserverScope {
    /// Creates a bridge observer scope anchored to the trusted parent of its
    /// first scanned block.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero bridge address or parent hash.
    pub fn new(
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        first_block: u64,
        first_parent_hash: EvmHash,
    ) -> Result<Self, EvmObserverError> {
        if bridge_contract.is_zero() {
            return Err(EvmObserverError::InvalidBridgeContract);
        }
        if first_parent_hash.is_zero() {
            return Err(EvmObserverError::InvalidParentHash);
        }
        Ok(Self {
            chain_id,
            bridge_contract,
            first_block,
            first_parent_hash,
        })
    }

    #[must_use]
    pub const fn initial_checkpoint(self) -> EvmObserverCheckpoint {
        EvmObserverCheckpoint {
            next_block: self.first_block,
            parent_hash: self.first_parent_hash,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvmObserverCheckpoint {
    pub next_block: u64,
    pub parent_hash: EvmHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedEvmObserverBlock {
    pub finalized: EvmFinalizedBlock,
    pub parent_hash: EvmHash,
    pub deposits: Vec<VerifiedEvmDeposit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedEvmObserverBlock {
    scope: EvmObserverScope,
    previous: EvmObserverCheckpoint,
    next: EvmObserverCheckpoint,
    finalized: EvmFinalizedBlock,
    deposits: Vec<VerifiedEvmDeposit>,
}

impl PreparedEvmObserverBlock {
    #[must_use]
    pub const fn scope(&self) -> EvmObserverScope {
        self.scope
    }

    #[must_use]
    pub const fn previous(&self) -> EvmObserverCheckpoint {
        self.previous
    }

    #[must_use]
    pub const fn next(&self) -> EvmObserverCheckpoint {
        self.next
    }

    #[must_use]
    pub const fn finalized(&self) -> EvmFinalizedBlock {
        self.finalized
    }

    #[must_use]
    pub fn deposits(&self) -> &[VerifiedEvmDeposit] {
        &self.deposits
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmObserverError {
    InvalidBridgeContract,
    InvalidParentHash,
    BlockSequenceMismatch,
    ParentHashMismatch,
    MissingFinalityProof,
    TooManyDeposits,
    WrongDepositNetwork,
    DepositBlockMismatch,
    DepositOrderMismatch,
    HeightOverflow,
}

/// Validates a complete finalized block before any observer cursor or deposit
/// record is changed.
///
/// # Errors
///
/// Returns an error for a gap, fork, missing evidence, oversized batch,
/// malformed deposit or arithmetic overflow.
pub fn prepare_observer_block(
    scope: EvmObserverScope,
    checkpoint: EvmObserverCheckpoint,
    block: FinalizedEvmObserverBlock,
) -> Result<PreparedEvmObserverBlock, EvmObserverError> {
    if block.finalized.number != checkpoint.next_block {
        return Err(EvmObserverError::BlockSequenceMismatch);
    }
    if block.parent_hash != checkpoint.parent_hash {
        return Err(EvmObserverError::ParentHashMismatch);
    }
    if block.finalized.proof_hash.iter().all(|byte| *byte == 0) {
        return Err(EvmObserverError::MissingFinalityProof);
    }
    if block.deposits.len() > MAX_DEPOSITS_PER_BLOCK {
        return Err(EvmObserverError::TooManyDeposits);
    }
    validate_deposits(scope, block.finalized, &block.deposits)?;
    let next_block = checkpoint
        .next_block
        .checked_add(1)
        .ok_or(EvmObserverError::HeightOverflow)?;
    Ok(PreparedEvmObserverBlock {
        scope,
        previous: checkpoint,
        next: EvmObserverCheckpoint {
            next_block,
            parent_hash: block.finalized.hash,
        },
        finalized: block.finalized,
        deposits: block.deposits,
    })
}

fn validate_deposits(
    scope: EvmObserverScope,
    finalized: EvmFinalizedBlock,
    deposits: &[VerifiedEvmDeposit],
) -> Result<(), EvmObserverError> {
    let expected_network = origin_network_id(scope.chain_id);
    let mut previous_index = None;
    for deposit in deposits {
        let claim = deposit.claim();
        let evidence = deposit.evidence();
        if claim.origin_network != expected_network {
            return Err(EvmObserverError::WrongDepositNetwork);
        }
        if evidence.block_height != finalized.number
            || evidence.block_hash != *finalized.hash.as_bytes()
            || evidence.proof_hash != finalized.proof_hash
        {
            return Err(EvmObserverError::DepositBlockMismatch);
        }
        if previous_index.is_some_and(|index| index >= claim.event_index) {
            return Err(EvmObserverError::DepositOrderMismatch);
        }
        previous_index = Some(claim.event_index);
    }
    Ok(())
}
