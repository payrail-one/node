use std::collections::BTreeSet;

use crate::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmChainId, EvmObservedBlock, EvmProviderId,
    MAX_DEPOSITS_PER_BLOCK,
};

pub trait EvmBlockObservationProvider {
    type Error;

    #[must_use]
    fn provider_id(&self) -> EvmProviderId;

    /// Reads one exact block and all deposit logs emitted by the requested
    /// bridge contract. `Ok(None)` means the provider does not have the block.
    ///
    /// # Errors
    ///
    /// Returns the provider-specific transport or decoding error.
    fn observed_block(
        &self,
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error>;
}

pub trait EvmBlockObservationSource {
    type Error;

    /// Returns one canonical candidate block only when the configured
    /// observation policy has established its complete ordered log set.
    ///
    /// # Errors
    ///
    /// Returns the source error when observation evidence conflicts or cannot
    /// be validated. `Ok(None)` means the source has no quorum yet.
    fn observed_block(
        &self,
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvmBlockObservationQuorumError {
    InvalidProviderCount,
    InvalidThreshold,
    DuplicateProvider,
    ProviderResponseMismatch,
    ProviderDisagreement,
}

#[derive(Clone, Debug)]
pub struct EvmBlockObservationQuorum<Provider> {
    providers: Vec<Provider>,
    minimum_agreement: usize,
}

impl<Provider: EvmBlockObservationProvider> EvmBlockObservationQuorum<Provider> {
    /// Builds a bounded independent-provider observation policy.
    ///
    /// At least two configured providers and two agreeing block observations
    /// are required. Any successful but conflicting response fails closed,
    /// even if another subset reaches the threshold.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe provider count or threshold, or repeated
    /// provider identities.
    pub fn new(
        providers: Vec<Provider>,
        minimum_agreement: usize,
    ) -> Result<Self, EvmBlockObservationQuorumError> {
        if !(2..=crate::MAX_FINALITY_PROVIDERS).contains(&providers.len()) {
            return Err(EvmBlockObservationQuorumError::InvalidProviderCount);
        }
        if !(2..=providers.len()).contains(&minimum_agreement) {
            return Err(EvmBlockObservationQuorumError::InvalidThreshold);
        }
        let mut identities = BTreeSet::new();
        if providers
            .iter()
            .any(|provider| !identities.insert(provider.provider_id()))
        {
            return Err(EvmBlockObservationQuorumError::DuplicateProvider);
        }
        Ok(Self {
            providers,
            minimum_agreement,
        })
    }

    /// Returns one exact block only after the configured provider quorum agrees
    /// on its hash, parent and complete ordered deposit-log collection.
    ///
    /// # Errors
    ///
    /// Returns an error when a provider returns internally inconsistent data or
    /// successful providers disagree.
    fn read_observed_block(
        &self,
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, EvmBlockObservationQuorumError> {
        let mut agreement = 0_usize;
        let mut agreed = None;
        for provider in &self.providers {
            let Ok(Some(block)) = provider.observed_block(chain_id, bridge_contract, block_number)
            else {
                continue;
            };
            validate_response(&block, bridge_contract, block_number)?;
            if agreed.as_ref().is_some_and(|expected| expected != &block) {
                return Err(EvmBlockObservationQuorumError::ProviderDisagreement);
            }
            agreed = Some(block);
            agreement = agreement.saturating_add(1);
        }
        if agreement < self.minimum_agreement {
            return Ok(None);
        }
        Ok(agreed)
    }
}

impl<Provider: EvmBlockObservationProvider> EvmBlockObservationSource
    for EvmBlockObservationQuorum<Provider>
{
    type Error = EvmBlockObservationQuorumError;

    fn observed_block(
        &self,
        chain_id: EvmChainId,
        bridge_contract: EvmAddress,
        block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error> {
        self.read_observed_block(chain_id, bridge_contract, block_number)
    }
}

fn validate_response(
    block: &EvmObservedBlock,
    bridge_contract: EvmAddress,
    block_number: u64,
) -> Result<(), EvmBlockObservationQuorumError> {
    if block.number != block_number
        || block.hash.is_zero()
        || block.parent_hash.is_zero()
        || block.logs.len() > MAX_DEPOSITS_PER_BLOCK
    {
        return Err(EvmBlockObservationQuorumError::ProviderResponseMismatch);
    }
    let mut previous_index = None;
    for log in &block.logs {
        if log.address != bridge_contract
            || log.removed
            || log.topics.first() != Some(&DEPOSITED_EVENT_TOPIC)
            || log.block_number != Some(block_number)
            || log.block_hash != Some(block.hash)
            || log.transaction_hash.is_none_or(crate::EvmHash::is_zero)
            || log.log_index.is_none()
            || previous_index.is_some_and(|index| log.log_index.is_some_and(|next| index >= next))
        {
            return Err(EvmBlockObservationQuorumError::ProviderResponseMismatch);
        }
        previous_index = log.log_index;
    }
    Ok(())
}
