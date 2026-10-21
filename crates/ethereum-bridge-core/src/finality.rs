use external_asset_core::FinalityEvidence;

use crate::{
    EvmChainId, EvmDepositVerificationError, EvmFinalizedBlock, EvmLog, VerifiedEvmDeposit,
    decode_deposit,
};

/// Read-only boundary for a component that establishes Ethereum canonical
/// finality. Implementations must not return a block based only on `latest`,
/// `safe`, a confirmation count, or an unverified single-provider response.
pub trait EvmFinalitySource {
    type Error;

    /// Returns the canonical block at `number` only when the source has
    /// established that the block is finalized under the configured policy.
    ///
    /// # Errors
    ///
    /// Returns the adapter error when finality evidence cannot be obtained or
    /// verified. `Ok(None)` means the requested block is not finalized.
    fn finalized_canonical_block(
        &self,
        chain_id: EvmChainId,
        number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error>;
}

/// Decodes a bridge deposit and binds it to the canonical finalized block at
/// the exact height reported by the receipt log.
///
/// # Errors
///
/// Returns a typed decoding, source, non-finality, canonicality or missing
/// evidence error. No claim is returned unless every check succeeds.
pub fn verify_finalized_deposit<Source: EvmFinalitySource>(
    binding: crate::EvmAssetBinding,
    log: &EvmLog,
    source: &Source,
) -> Result<VerifiedEvmDeposit, EvmDepositVerificationError<Source::Error>> {
    let observed = decode_deposit(binding, log).map_err(EvmDepositVerificationError::Deposit)?;
    let finalized = source
        .finalized_canonical_block(binding.chain_id(), observed.block_number)
        .map_err(EvmDepositVerificationError::Source)?
        .ok_or(EvmDepositVerificationError::BlockNotFinalized)?;
    if finalized.number != observed.block_number {
        return Err(EvmDepositVerificationError::FinalizedBlockNumberMismatch);
    }
    if finalized.hash != observed.block_hash {
        return Err(EvmDepositVerificationError::NonCanonicalBlock);
    }
    if finalized.proof_hash.iter().all(|byte| *byte == 0) {
        return Err(EvmDepositVerificationError::MissingFinalityProof);
    }
    Ok(VerifiedEvmDeposit {
        claim: observed.claim,
        evidence: FinalityEvidence {
            block_height: finalized.number,
            block_hash: *finalized.hash.as_bytes(),
            proof_hash: finalized.proof_hash,
        },
    })
}
