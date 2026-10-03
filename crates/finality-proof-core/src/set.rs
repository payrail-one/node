use std::collections::BTreeSet;

use ledger_core::NetworkId;
use network_membership_core::{ConsensusPublicKey, NodeId};
use sha2::{Digest, Sha256};
use state_sync_core::ValidatorSetHash;

use crate::{FinalityError, Validator};

pub const MAX_VALIDATORS: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatorSet {
    network: NetworkId,
    membership_epoch: u64,
    validators: Vec<Validator>,
    total_weight: u64,
    quorum_weight: u64,
    hash: ValidatorSetHash,
}

impl ValidatorSet {
    /// Constructs a canonical weighted validator set and its content hash.
    ///
    /// # Errors
    ///
    /// Returns an error for empty/oversized sets, non-increasing node order,
    /// duplicate keys, zero weights or total-weight overflow.
    pub fn new(
        network: NetworkId,
        membership_epoch: u64,
        validators: Vec<Validator>,
    ) -> Result<Self, FinalityError> {
        if validators.is_empty() {
            return Err(FinalityError::EmptyValidatorSet);
        }
        if validators.len() > MAX_VALIDATORS {
            return Err(FinalityError::TooManyValidators);
        }
        let validator_count =
            u32::try_from(validators.len()).map_err(|_| FinalityError::TooManyValidators)?;
        let mut previous: Option<NodeId> = None;
        let mut keys = BTreeSet::<ConsensusPublicKey>::new();
        let mut total_weight = 0_u64;
        for validator in &validators {
            if previous.is_some_and(|node| node >= validator.node) {
                return Err(FinalityError::NonCanonicalValidatorOrder);
            }
            if !keys.insert(validator.consensus_key) {
                return Err(FinalityError::DuplicateConsensusKey);
            }
            if validator.weight == 0 {
                return Err(FinalityError::ZeroValidatorWeight);
            }
            total_weight = total_weight
                .checked_add(validator.weight)
                .ok_or(FinalityError::TotalWeightOverflow)?;
            previous = Some(validator.node);
        }
        let quorum_weight = total_weight - ((total_weight - 1) / 3);
        let hash = hash_set(network, membership_epoch, validator_count, &validators);
        Ok(Self {
            network,
            membership_epoch,
            validators,
            total_weight,
            quorum_weight,
            hash,
        })
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn membership_epoch(&self) -> u64 {
        self.membership_epoch
    }

    #[must_use]
    pub const fn hash(&self) -> ValidatorSetHash {
        self.hash
    }

    #[must_use]
    pub const fn total_weight(&self) -> u64 {
        self.total_weight
    }

    #[must_use]
    pub const fn quorum_weight(&self) -> u64 {
        self.quorum_weight
    }

    #[must_use]
    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    pub(crate) fn validator(&self, node: NodeId) -> Option<Validator> {
        self.validators
            .binary_search_by_key(&node, |validator| validator.node)
            .ok()
            .map(|index| self.validators[index])
    }
}

fn hash_set(
    network: NetworkId,
    membership_epoch: u64,
    validator_count: u32,
    validators: &[Validator],
) -> ValidatorSetHash {
    let mut hasher = Sha256::new();
    hasher.update(b"finality.validator-set\0");
    hasher.update(network.as_bytes());
    hasher.update(membership_epoch.to_be_bytes());
    hasher.update(validator_count.to_be_bytes());
    for validator in validators {
        hasher.update(validator.node.as_bytes());
        hasher.update(validator.consensus_key.as_bytes());
        hasher.update(validator.weight.to_be_bytes());
    }
    ValidatorSetHash::new(hasher.finalize().into())
}
