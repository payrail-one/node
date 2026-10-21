use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::{
    EvmChainId, EvmFinalitySource, EvmFinalizedBlock, EvmHash, EvmProviderFinality, EvmProviderId,
};

pub const MAX_FINALITY_PROVIDERS: usize = 8;

pub trait EvmFinalityProvider {
    type Error;

    #[must_use]
    fn provider_id(&self) -> EvmProviderId;

    /// Reads the requested canonical block and the provider's finalized head as
    /// one internally consistent view. `Ok(None)` means the provider has not
    /// finalized the requested height yet.
    ///
    /// # Errors
    ///
    /// Returns the provider-specific transport, decoding or verification error.
    fn finalized_view(
        &self,
        chain_id: EvmChainId,
        block_number: u64,
    ) -> Result<Option<EvmProviderFinality>, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmFinalityQuorumError {
    InvalidProviderCount,
    InvalidThreshold,
    DuplicateProvider,
    ProviderResponseMismatch,
    ProviderDisagreement,
    NoQuorum,
}

#[derive(Clone, Debug)]
pub struct EvmFinalityQuorum<Provider> {
    providers: Vec<Provider>,
    minimum_agreement: usize,
    policy_digest: [u8; 32],
}

impl<Provider: EvmFinalityProvider> EvmFinalityQuorum<Provider> {
    /// Builds a bounded independent-provider finality policy.
    ///
    /// At least two configured providers and two agreeing finalized views are
    /// required. Provider identifiers are stable operational identities, not
    /// URLs, and must be unique.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe provider count or threshold, or repeated
    /// provider identities.
    pub fn new(
        providers: Vec<Provider>,
        minimum_agreement: usize,
    ) -> Result<Self, EvmFinalityQuorumError> {
        if !(2..=MAX_FINALITY_PROVIDERS).contains(&providers.len()) {
            return Err(EvmFinalityQuorumError::InvalidProviderCount);
        }
        if !(2..=providers.len()).contains(&minimum_agreement) {
            return Err(EvmFinalityQuorumError::InvalidThreshold);
        }
        let mut identities = BTreeSet::new();
        if providers
            .iter()
            .any(|provider| !identities.insert(provider.provider_id()))
        {
            return Err(EvmFinalityQuorumError::DuplicateProvider);
        }
        Ok(Self {
            providers,
            minimum_agreement,
            policy_digest: policy_digest(&identities, minimum_agreement),
        })
    }
}

impl<Provider: EvmFinalityProvider> EvmFinalitySource for EvmFinalityQuorum<Provider> {
    type Error = EvmFinalityQuorumError;

    fn finalized_canonical_block(
        &self,
        chain_id: EvmChainId,
        number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error> {
        let mut agreeing = 0_usize;
        let mut canonical_hash = None;
        let mut finalized_heads = std::collections::BTreeMap::new();
        for provider in &self.providers {
            let Ok(Some(view)) = provider.finalized_view(chain_id, number) else {
                continue;
            };
            if view.block_number != number || view.finalized_head_number < number {
                return Err(EvmFinalityQuorumError::ProviderResponseMismatch);
            }
            if canonical_hash.is_some_and(|hash| hash != view.block_hash) {
                return Err(EvmFinalityQuorumError::ProviderDisagreement);
            }
            if finalized_heads
                .insert(view.finalized_head_number, view.finalized_head_hash)
                .is_some_and(|hash| hash != view.finalized_head_hash)
            {
                return Err(EvmFinalityQuorumError::ProviderDisagreement);
            }
            canonical_hash = Some(view.block_hash);
            agreeing = agreeing.saturating_add(1);
        }
        if agreeing < self.minimum_agreement {
            return Ok(None);
        }
        let block_hash = canonical_hash.ok_or(EvmFinalityQuorumError::NoQuorum)?;
        Ok(Some(EvmFinalizedBlock {
            number,
            hash: block_hash,
            proof_hash: evidence_commitment(chain_id, number, block_hash, self.policy_digest),
        }))
    }
}

fn policy_digest(providers: &BTreeSet<EvmProviderId>, minimum_agreement: usize) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"payrail.ethereum.finality-policy\0");
    let threshold = u64::try_from(minimum_agreement).unwrap_or(u64::MAX);
    hasher.update(threshold.to_be_bytes());
    for provider in providers {
        hasher.update(provider.as_bytes());
    }
    hasher.finalize().into()
}

fn evidence_commitment(
    chain_id: EvmChainId,
    block_number: u64,
    block_hash: EvmHash,
    policy_digest: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"payrail.ethereum.finality-quorum\0");
    hasher.update(chain_id.get().to_be_bytes());
    hasher.update(block_number.to_be_bytes());
    hasher.update(block_hash.as_bytes());
    hasher.update(policy_digest);
    hasher.finalize().into()
}
