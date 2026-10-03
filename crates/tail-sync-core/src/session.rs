use ledger_core::NetworkId;
use state_sync_core::{FinalityProofVerifier, FinalizedCheckpoint, MAX_FINALITY_PROOF_BYTES};

use crate::{
    FinalizedTailBlock, MAX_TAIL_PAYLOAD_BYTES, PreparedTailTransition, TailSyncError,
    TailTransitionExecutor, VerifiedTailBlock,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TailSyncSession {
    network: NetworkId,
    current: FinalizedCheckpoint,
}

impl TailSyncSession {
    #[must_use]
    pub const fn new(network: NetworkId, current: FinalizedCheckpoint) -> Self {
        Self { network, current }
    }

    #[must_use]
    pub const fn current(&self) -> FinalizedCheckpoint {
        self.current
    }

    /// Verifies the next finalized block without advancing the local cursor.
    /// The caller can durably publish the prepared state before cursor commit.
    ///
    /// # Errors
    ///
    /// Returns an error for a gap, fork, oversized input, invalid finality or a
    /// transition whose computed commitments do not match the checkpoint.
    pub fn verify_next<F: FinalityProofVerifier, T: TailTransitionExecutor>(
        &self,
        block: FinalizedTailBlock,
        previous_state: &[u8],
        finality: &F,
        transition: &T,
    ) -> Result<VerifiedTailBlock, TailSyncError> {
        self.verify_next_with(block, finality, |previous, payload| {
            transition
                .prepare_transition(previous, previous_state, payload)
                .map(|prepared| (prepared, ()))
        })
        .map(|(verified, ())| verified)
    }

    /// Verifies finality, then invokes a side-effect-free transition preparer.
    ///
    /// The auxiliary value is returned only when the prepared commitment matches
    /// the finalized checkpoint. This lets a runtime retain an opaque database or
    /// authenticated-state update without executing before finality is checked.
    ///
    /// # Errors
    ///
    /// Returns an error for a gap, fork, oversized input, invalid finality or
    /// commitments that do not match the finalized checkpoint, or a preparer
    /// that rejects the transition.
    pub fn verify_next_with<F, A, P>(
        &self,
        block: FinalizedTailBlock,
        finality: &F,
        prepare: P,
    ) -> Result<(VerifiedTailBlock, A), TailSyncError>
    where
        F: FinalityProofVerifier,
        P: FnOnce(FinalizedCheckpoint, &[u8]) -> Option<(PreparedTailTransition, A)>,
    {
        self.validate_block(&block, finality)?;
        let Some((prepared, auxiliary)) = prepare(self.current, block.payload.as_slice()) else {
            return Err(TailSyncError::InvalidTransition);
        };
        let verified = self.finish_verification(block, prepared.commitment, prepared.state)?;
        Ok((verified, auxiliary))
    }

    fn validate_block<F: FinalityProofVerifier>(
        &self,
        block: &FinalizedTailBlock,
        finality: &F,
    ) -> Result<(), TailSyncError> {
        if block.network != self.network {
            return Err(TailSyncError::WrongNetwork);
        }
        let expected_height = self
            .current
            .height
            .checked_add(1)
            .ok_or(TailSyncError::HeightOverflow)?;
        if block.checkpoint.height != expected_height {
            return Err(TailSyncError::NonSequentialHeight);
        }
        if block.parent_hash != self.current.block_hash {
            return Err(TailSyncError::ParentHashMismatch);
        }
        if block.payload.len() > MAX_TAIL_PAYLOAD_BYTES {
            return Err(TailSyncError::PayloadTooLarge);
        }
        if block.finality_proof.len() > MAX_FINALITY_PROOF_BYTES {
            return Err(TailSyncError::FinalityProofTooLarge);
        }
        if !finality.verify(block.network, block.checkpoint, &block.finality_proof) {
            return Err(TailSyncError::InvalidFinalityProof);
        }
        Ok(())
    }

    fn finish_verification(
        &self,
        block: FinalizedTailBlock,
        commitment: crate::TailCommitment,
        state: Vec<u8>,
    ) -> Result<VerifiedTailBlock, TailSyncError> {
        if commitment.block_hash != block.checkpoint.block_hash
            || commitment.state_root != block.checkpoint.state_root
        {
            return Err(TailSyncError::InvalidTransition);
        }
        Ok(VerifiedTailBlock {
            network: block.network,
            previous: self.current,
            checkpoint: block.checkpoint,
            payload: block.payload,
            state,
        })
    }

    /// Advances the finalized cursor after the caller durably applies the
    /// already verified payload.
    ///
    /// # Errors
    ///
    /// Returns an error if another block advanced the session after verification.
    pub fn commit(&mut self, verified: &VerifiedTailBlock) -> Result<(), TailSyncError> {
        if verified.previous != self.current {
            return Err(TailSyncError::StaleVerifiedBlock);
        }
        self.current = verified.checkpoint;
        Ok(())
    }
}
