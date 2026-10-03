use ledger_core::{Ledger, LedgerSnapshot, NetworkId, OperationReceipt, SignatureVerifier};
use state_sync_core::{BlockHash, FinalizedCheckpoint};
use tail_sync_core::{PreparedTailTransition, TailCommitment, TailTransitionExecutor};
use transaction_verification_core::SharedVerifiedOperationCache;

use crate::{
    LedgerAuthenticatedState, LedgerBlockCodec, LedgerStateCodec,
    PreparedLedgerAuthenticatedUpdate, RuntimeError, SignatureVerificationPolicy, block_hash,
    state_commitment::{StateCommitmentPolicy, StateCommitmentScheme},
    state_root,
    verification::verify_operations,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutedLedgerBlock {
    pub transition: PreparedTailTransition,
    pub receipts: Vec<OperationReceipt>,
}

/// A fully executed block plus its side-effect-free authenticated-state update.
pub struct PreparedIncrementalLedgerBlock {
    executed: ExecutedLedgerBlock,
    authenticated_update: PreparedLedgerAuthenticatedUpdate,
}

impl PreparedIncrementalLedgerBlock {
    #[must_use]
    pub const fn executed(&self) -> &ExecutedLedgerBlock {
        &self.executed
    }

    #[must_use]
    pub fn into_parts(self) -> (ExecutedLedgerBlock, PreparedLedgerAuthenticatedUpdate) {
        (self.executed, self.authenticated_update)
    }
}

struct CandidateExecution {
    snapshot: LedgerSnapshot,
    state: Vec<u8>,
    receipts: Vec<OperationReceipt>,
    next_height: u64,
    block_hash: BlockHash,
}

impl CandidateExecution {
    fn finish(self, state_root: state_sync_core::StateRoot) -> ExecutedLedgerBlock {
        ExecutedLedgerBlock {
            transition: PreparedTailTransition {
                commitment: TailCommitment {
                    block_hash: self.block_hash,
                    state_root,
                },
                state: self.state,
            },
            receipts: self.receipts,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LedgerBlockExecutor<V> {
    network: NetworkId,
    pub(crate) verifier: V,
    pub(crate) signature_policy: SignatureVerificationPolicy,
    pub(crate) state_commitment_policy: StateCommitmentPolicy,
    pub(crate) authorization_cache: Option<SharedVerifiedOperationCache>,
}

impl<V> LedgerBlockExecutor<V> {
    #[must_use]
    pub fn new(network: NetworkId, verifier: V) -> Self {
        Self {
            network,
            verifier,
            signature_policy: SignatureVerificationPolicy::default(),
            state_commitment_policy: StateCommitmentPolicy::default(),
            authorization_cache: None,
        }
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn signature_policy(&self) -> SignatureVerificationPolicy {
        self.signature_policy
    }

    #[must_use]
    pub const fn state_commitment_policy(&self) -> StateCommitmentPolicy {
        self.state_commitment_policy
    }

    #[must_use]
    pub const fn with_signature_policy(mut self, policy: SignatureVerificationPolicy) -> Self {
        self.signature_policy = policy;
        self
    }

    #[must_use]
    pub const fn with_state_commitment_policy(mut self, policy: StateCommitmentPolicy) -> Self {
        self.state_commitment_policy = policy;
        self
    }

    /// Reuses authorizations independently verified earlier in this process.
    /// A wrong-network cache is ignored and all misses follow the normal
    /// verification path.
    #[must_use]
    pub fn with_authorization_cache(mut self, cache: SharedVerifiedOperationCache) -> Self {
        if cache.network() == self.network {
            self.authorization_cache = Some(cache);
        }
        self
    }
}

impl<V: SignatureVerifier + Sync> LedgerBlockExecutor<V> {
    /// Executes a complete block against an isolated restored ledger.
    ///
    /// The caller's previous state is immutable. No result is publishable until
    /// every envelope, signature and ledger transition has succeeded.
    ///
    /// # Errors
    ///
    /// Returns an error for a corrupt previous state, root mismatch, malformed
    /// block, invalid signature or any failed monetary operation.
    pub fn execute(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
    ) -> Result<ExecutedLedgerBlock, RuntimeError> {
        let previous_snapshot = LedgerStateCodec::decode(self.network, previous_state)?;
        if self.state_commitment_policy.root_for_snapshot(
            previous.height,
            previous_state,
            &previous_snapshot,
        )? != previous.state_root
        {
            return Err(RuntimeError::PreviousStateRootMismatch);
        }
        let candidate = self.execute_candidate(previous, previous_snapshot, payload)?;
        let root = self.state_commitment_policy.root_for_snapshot(
            candidate.next_height,
            &candidate.state,
            &candidate.snapshot,
        )?;
        Ok(candidate.finish(root))
    }

    /// Executes a block and prepares only the authenticated row delta.
    ///
    /// Neither the supplied authenticated view nor the caller's previous state
    /// is changed. The returned update can be committed only against the exact
    /// view used here.
    ///
    /// # Errors
    ///
    /// Returns an error for a mismatched authenticated predecessor, corrupt
    /// state, malformed block, invalid signature or failed monetary operation.
    pub fn execute_incremental(
        &self,
        previous: FinalizedCheckpoint,
        previous_state: &[u8],
        payload: &[u8],
        authenticated: &LedgerAuthenticatedState,
    ) -> Result<PreparedIncrementalLedgerBlock, RuntimeError> {
        let previous_snapshot = LedgerStateCodec::decode(self.network, previous_state)?;
        authenticated.validate_snapshot(previous.height, &previous_snapshot)?;
        let previous_root = match self.state_commitment_policy.scheme_at(previous.height) {
            StateCommitmentScheme::CanonicalStateV1 => state_root(previous_state),
            StateCommitmentScheme::AuthenticatedStateV1 => authenticated.root(),
        };
        if previous_root != previous.state_root {
            return Err(RuntimeError::PreviousStateRootMismatch);
        }
        let candidate = self.execute_candidate(previous, previous_snapshot, payload)?;
        let authenticated_update =
            authenticated.prepare(candidate.next_height, &candidate.snapshot)?;
        let root = match self
            .state_commitment_policy
            .scheme_at(candidate.next_height)
        {
            StateCommitmentScheme::CanonicalStateV1 => state_root(&candidate.state),
            StateCommitmentScheme::AuthenticatedStateV1 => authenticated_update.root(),
        };
        Ok(PreparedIncrementalLedgerBlock {
            executed: candidate.finish(root),
            authenticated_update,
        })
    }

    fn execute_candidate(
        &self,
        previous: FinalizedCheckpoint,
        previous_snapshot: LedgerSnapshot,
        payload: &[u8],
    ) -> Result<CandidateExecution, RuntimeError> {
        let next_height = previous
            .height
            .checked_add(1)
            .ok_or(RuntimeError::InvalidStateHeight)?;
        let mut candidate = Ledger::from_snapshot(self.network, previous_snapshot)
            .map_err(RuntimeError::InvalidSnapshot)?;
        let operations = LedgerBlockCodec::decode(payload)?;
        let verified = verify_operations(
            self.network,
            &self.verifier,
            operations,
            self.signature_policy,
            self.authorization_cache.as_deref(),
        )?;
        let mut receipts = Vec::with_capacity(verified.len());
        for (index, operation) in verified.into_iter().enumerate() {
            let operation_index =
                u32::try_from(index).map_err(|_| RuntimeError::TooManyOperations)?;
            let operation = operation.map_err(|source| RuntimeError::ExecutionFailed {
                operation_index,
                source,
            })?;
            let receipt = candidate
                .submit_verified_at_height(operation, next_height)
                .map_err(|source| RuntimeError::ExecutionFailed {
                    operation_index,
                    source,
                })?;
            receipts.push(receipt);
        }
        let snapshot = candidate.snapshot();
        let state = LedgerStateCodec::encode(&snapshot)?;
        Ok(CandidateExecution {
            snapshot,
            state,
            receipts,
            next_height,
            block_hash: block_hash(previous.block_hash, payload),
        })
    }
}

impl<V: SignatureVerifier + Sync> TailTransitionExecutor for LedgerBlockExecutor<V> {
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
