use ledger_core::{
    AuthorizedOperation, Ledger, LedgerError, OperationId, SignatureVerifier, SignedOperation,
    VerifiedOperation,
};
use state_sync_core::FinalizedCheckpoint;
use tail_sync_core::MAX_TAIL_PAYLOAD_BYTES;
use transaction_protocol::{MAX_ENVELOPE_BYTES, SignedOperationCodec};

use crate::{
    ExecutedLedgerBlock, LedgerAuthenticatedState, LedgerBlockCodec, LedgerBlockExecutor,
    LedgerStateCodec, MAX_BLOCK_OPERATIONS, PreparedLedgerAuthenticatedUpdate, RuntimeError,
    verification::verify_operations,
};

const BLOCK_HEADER_BYTES: usize = 20;
const ENVELOPE_LENGTH_BYTES: usize = 4;
pub const MAX_PROPOSAL_CANDIDATES: usize = 16_384;
pub const MAX_PROPOSAL_PASSES: usize = 4;
pub const DEFAULT_MAX_EXPIRED_OPERATIONS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockCandidate {
    pub operation: OperationId,
    pub envelope: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposalLimits {
    pub max_candidates: usize,
    pub max_operations: usize,
    pub max_expired_operations: usize,
    pub max_payload_bytes: usize,
    pub max_passes: usize,
}

impl Default for ProposalLimits {
    fn default() -> Self {
        Self {
            max_candidates: MAX_PROPOSAL_CANDIDATES,
            max_operations: MAX_BLOCK_OPERATIONS,
            max_expired_operations: DEFAULT_MAX_EXPIRED_OPERATIONS,
            max_payload_bytes: MAX_TAIL_PAYLOAD_BYTES,
            max_passes: MAX_PROPOSAL_PASSES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalRejection {
    InvalidEnvelope,
    OperationIdMismatch,
    PermanentLedgerFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedCandidate {
    pub operation: OperationId,
    pub reason: ProposalRejection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockProposal {
    pub payload: Vec<u8>,
    pub included: Vec<OperationId>,
    pub deferred: Vec<OperationId>,
    pub rejected: Vec<RejectedCandidate>,
    pub executed: ExecutedLedgerBlock,
}

/// A proposal executed against an incremental authenticated-state view.
pub struct PreparedIncrementalBlockProposal {
    proposal: BlockProposal,
    authenticated_update: PreparedLedgerAuthenticatedUpdate,
}

impl PreparedIncrementalBlockProposal {
    #[must_use]
    pub const fn proposal(&self) -> &BlockProposal {
        &self.proposal
    }

    #[must_use]
    pub fn into_parts(self) -> (BlockProposal, PreparedLedgerAuthenticatedUpdate) {
        (self.proposal, self.authenticated_update)
    }
}

struct ProposalSelection {
    payload: Vec<u8>,
    included: Vec<OperationId>,
    deferred: Vec<OperationId>,
    rejected: Vec<RejectedCandidate>,
    expected_state: Vec<u8>,
}

impl ProposalSelection {
    fn finish(self, executed: ExecutedLedgerBlock) -> Result<BlockProposal, RuntimeError> {
        if executed.transition.state != self.expected_state {
            return Err(RuntimeError::CandidateOperationMismatch);
        }
        Ok(BlockProposal {
            payload: self.payload,
            included: self.included,
            deferred: self.deferred,
            rejected: self.rejected,
            executed,
        })
    }
}

struct PreparedCandidate {
    operation: OperationId,
    envelope_bytes: usize,
    verified: VerifiedOperation,
}

struct DecodedCandidate {
    operation: OperationId,
    envelope_bytes: usize,
    signed: SignedOperation,
}

impl<V: SignatureVerifier + Sync> LedgerBlockExecutor<V> {
    /// Builds and executes a bounded deterministic proposal from canonical candidates.
    ///
    /// Candidates must be strictly ordered by operation ID. Executable operations
    /// are scheduled by sender, nonce and operation ID. Deferred operations get a
    /// bounded number of passes so incoming-payment dependencies can resolve without
    /// permitting quadratic unbounded work.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid limits, non-canonical candidates, invalid
    /// previous state or a proposal that cannot be reproduced by the block executor.
    pub fn build_proposal(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        candidates: &[BlockCandidate],
        limits: ProposalLimits,
    ) -> Result<BlockProposal, RuntimeError> {
        let selection = self.select_proposal(previous, previous_state, candidates, limits)?;
        let executed = self.execute(previous, previous_state, &selection.payload)?;
        selection.finish(executed)
    }

    /// Builds a proposal and prepares its authenticated row delta without
    /// rebuilding the complete authenticated tree.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid limits, candidates or predecessor state,
    /// or when runtime execution cannot reproduce deterministic selection.
    pub fn build_proposal_incremental(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        authenticated: &LedgerAuthenticatedState,
        candidates: &[BlockCandidate],
        limits: ProposalLimits,
    ) -> Result<PreparedIncrementalBlockProposal, RuntimeError> {
        let selection = self.select_proposal(previous, previous_state, candidates, limits)?;
        let prepared =
            self.execute_incremental(previous, previous_state, &selection.payload, authenticated)?;
        let (executed, authenticated_update) = prepared.into_parts();
        Ok(PreparedIncrementalBlockProposal {
            proposal: selection.finish(executed)?,
            authenticated_update,
        })
    }

    fn select_proposal(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        candidates: &[BlockCandidate],
        limits: ProposalLimits,
    ) -> Result<ProposalSelection, RuntimeError> {
        validate_limits(limits)?;
        validate_candidate_order(candidates, limits.max_candidates)?;
        let next_height = previous
            .height
            .checked_add(1)
            .ok_or(RuntimeError::InvalidStateHeight)?;
        let previous_snapshot = LedgerStateCodec::decode(self.network(), previous_state)?;
        let mut candidate_ledger = Ledger::from_snapshot(self.network(), previous_snapshot)
            .map_err(RuntimeError::InvalidSnapshot)?;
        let (mut decoded, mut rejected) = decode_candidates(candidates);
        decoded.sort_by_key(decoded_candidate_order_key);
        let (mut pending, authorization_rejections) = authorize_candidates(
            self.network(),
            &self.verifier,
            self.signature_policy,
            self.authorization_cache.as_deref(),
            decoded,
        )?;
        rejected.extend(authorization_rejections);
        pending.sort_by_key(candidate_order_key);

        let mut included_ids = Vec::new();
        let mut included_operations = Vec::new();
        let mut included_expired = 0_usize;
        let mut payload_bytes = BLOCK_HEADER_BYTES;
        for _ in 0..limits.max_passes {
            if pending.is_empty() || included_operations.len() == limits.max_operations {
                break;
            }
            let mut next = Vec::new();
            let mut progress = false;
            for candidate in pending {
                let is_expired = candidate.verified.operation().valid_until_height() < next_height;
                if included_operations.len() == limits.max_operations
                    || (is_expired && included_expired >= limits.max_expired_operations)
                    || !fits_payload(payload_bytes, candidate.envelope_bytes, limits)?
                {
                    next.push(candidate);
                    continue;
                }
                match candidate_ledger
                    .submit_verified_at_height(candidate.verified.clone(), next_height)
                {
                    Ok(_) => {
                        payload_bytes = payload_bytes
                            .checked_add(ENVELOPE_LENGTH_BYTES + candidate.envelope_bytes)
                            .ok_or(RuntimeError::ProposalSizeOverflow)?;
                        included_ids.push(candidate.operation);
                        included_operations.push(candidate.verified.into_signed());
                        if is_expired {
                            included_expired += 1;
                        }
                        progress = true;
                    }
                    Err(error) if permanent_failure(&error) => {
                        rejected.push(RejectedCandidate {
                            operation: candidate.operation,
                            reason: ProposalRejection::PermanentLedgerFailure,
                        });
                    }
                    Err(_) => next.push(candidate),
                }
            }
            pending = next;
            if !progress {
                break;
            }
        }
        let payload = LedgerBlockCodec::encode(&included_operations)?;
        if payload.len() > limits.max_payload_bytes {
            return Err(RuntimeError::ProposalSizeOverflow);
        }
        let expected_state = LedgerStateCodec::encode(&candidate_ledger.snapshot())?;
        Ok(ProposalSelection {
            payload,
            included: included_ids,
            deferred: pending
                .into_iter()
                .map(|candidate| candidate.operation)
                .collect(),
            rejected,
            expected_state,
        })
    }
}

fn validate_limits(limits: ProposalLimits) -> Result<(), RuntimeError> {
    if limits.max_candidates == 0
        || limits.max_candidates > MAX_PROPOSAL_CANDIDATES
        || limits.max_operations == 0
        || limits.max_operations > MAX_BLOCK_OPERATIONS
        || limits.max_expired_operations == 0
        || limits.max_expired_operations > MAX_BLOCK_OPERATIONS
        || limits.max_expired_operations > limits.max_operations
        || (limits.max_operations > 1 && limits.max_expired_operations == limits.max_operations)
        || limits.max_payload_bytes < BLOCK_HEADER_BYTES
        || limits.max_payload_bytes > MAX_TAIL_PAYLOAD_BYTES
        || limits.max_passes == 0
        || limits.max_passes > MAX_PROPOSAL_PASSES
    {
        return Err(RuntimeError::InvalidProposalLimits);
    }
    Ok(())
}

fn validate_candidate_order(
    candidates: &[BlockCandidate],
    maximum: usize,
) -> Result<(), RuntimeError> {
    if candidates.len() > maximum {
        return Err(RuntimeError::TooManyOperations);
    }
    if candidates
        .windows(2)
        .any(|pair| pair[0].operation >= pair[1].operation)
    {
        return Err(RuntimeError::NonCanonicalCandidates);
    }
    Ok(())
}

fn decode_candidates(
    candidates: &[BlockCandidate],
) -> (Vec<DecodedCandidate>, Vec<RejectedCandidate>) {
    let mut prepared = Vec::with_capacity(candidates.len());
    let mut rejected = Vec::new();
    for candidate in candidates {
        let Ok(signed) = SignedOperationCodec::decode(&candidate.envelope) else {
            rejected.push(RejectedCandidate {
                operation: candidate.operation,
                reason: ProposalRejection::InvalidEnvelope,
            });
            continue;
        };
        let Ok(actual) = signed.operation.operation_id() else {
            rejected.push(RejectedCandidate {
                operation: candidate.operation,
                reason: ProposalRejection::InvalidEnvelope,
            });
            continue;
        };
        if actual != candidate.operation {
            rejected.push(RejectedCandidate {
                operation: candidate.operation,
                reason: ProposalRejection::OperationIdMismatch,
            });
            continue;
        }
        prepared.push(DecodedCandidate {
            operation: actual,
            envelope_bytes: candidate.envelope.len(),
            signed,
        });
    }
    (prepared, rejected)
}

fn candidate_order_key(candidate: &PreparedCandidate) -> ([u8; 32], u64, [u8; 32]) {
    (
        *candidate.verified.operation().sender().as_bytes(),
        operation_nonce(candidate.verified.operation()),
        *candidate.operation.as_bytes(),
    )
}

fn decoded_candidate_order_key(candidate: &DecodedCandidate) -> ([u8; 32], u64, [u8; 32]) {
    (
        *candidate.signed.operation.sender().as_bytes(),
        operation_nonce(&candidate.signed.operation),
        *candidate.operation.as_bytes(),
    )
}

fn authorize_candidates<V: SignatureVerifier + Sync>(
    network: ledger_core::NetworkId,
    verifier: &V,
    policy: crate::SignatureVerificationPolicy,
    cache: Option<&transaction_verification_core::VerifiedOperationCache>,
    candidates: Vec<DecodedCandidate>,
) -> Result<(Vec<PreparedCandidate>, Vec<RejectedCandidate>), RuntimeError> {
    let mut metadata = Vec::with_capacity(candidates.len());
    let mut operations = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        metadata.push((candidate.operation, candidate.envelope_bytes));
        operations.push(candidate.signed);
    }
    let verification = verify_operations(network, verifier, operations, policy, cache)?;
    let mut prepared = Vec::with_capacity(verification.len());
    let mut rejected = Vec::new();
    for ((operation, envelope_bytes), result) in metadata.into_iter().zip(verification) {
        match result {
            Ok(checked) => prepared.push(PreparedCandidate {
                operation,
                envelope_bytes,
                verified: checked,
            }),
            Err(_) => rejected.push(RejectedCandidate {
                operation,
                reason: ProposalRejection::PermanentLedgerFailure,
            }),
        }
    }
    Ok((prepared, rejected))
}

const fn operation_nonce(operation: &AuthorizedOperation) -> u64 {
    match operation {
        AuthorizedOperation::Transfer(transfer)
        | AuthorizedOperation::SponsoredTransfer { transfer, .. } => transfer.nonce,
        AuthorizedOperation::TransferBatch(batch)
        | AuthorizedOperation::SponsoredBatchTransfer { batch, .. } => batch.nonce,
        AuthorizedOperation::ContractDeploy(deploy) => deploy.nonce,
        AuthorizedOperation::ContractCall(call) => call.nonce,
    }
}

fn fits_payload(
    current: usize,
    envelope: usize,
    limits: ProposalLimits,
) -> Result<bool, RuntimeError> {
    if envelope > MAX_ENVELOPE_BYTES {
        return Ok(false);
    }
    current
        .checked_add(ENVELOPE_LENGTH_BYTES)
        .and_then(|size| size.checked_add(envelope))
        .map(|size| size <= limits.max_payload_bytes)
        .ok_or(RuntimeError::ProposalSizeOverflow)
}

const fn permanent_failure(error: &LedgerError) -> bool {
    matches!(
        error,
        LedgerError::WrongNetwork
            | LedgerError::InvalidAuthorizationSet
            | LedgerError::InvalidSignature
            | LedgerError::BatchTooLarge
            | LedgerError::EmptyBatch
            | LedgerError::InvalidAmount
            | LedgerError::FeeSponsorshipNotApplicable
            | LedgerError::SameAccount
    )
}
