use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmBlockObservationProvider, EvmBlockObservationQuorum,
    EvmBlockObservationQuorumError, EvmBlockObservationSource, EvmChainId, EvmHash, EvmLog,
    EvmObservedBlock, EvmProviderId,
};

#[derive(Clone)]
struct Provider {
    id: u8,
    response: Result<Option<EvmObservedBlock>, ()>,
}

impl EvmBlockObservationProvider for Provider {
    type Error = ();

    fn provider_id(&self) -> EvmProviderId {
        EvmProviderId::new([self.id; 32])
    }

    fn observed_block(
        &self,
        _chain_id: EvmChainId,
        _bridge_contract: EvmAddress,
        _block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error> {
        self.response.clone()
    }
}

fn block(hash_byte: u8) -> EvmObservedBlock {
    EvmObservedBlock {
        number: 42,
        hash: EvmHash::new([hash_byte; 32]),
        parent_hash: EvmHash::new([8; 32]),
        logs: vec![EvmLog {
            address: EvmAddress::new([7; 20]),
            topics: vec![DEPOSITED_EVENT_TOPIC],
            data: vec![],
            transaction_hash: Some(EvmHash::new([3; 32])),
            block_hash: Some(EvmHash::new([hash_byte; 32])),
            block_number: Some(42),
            log_index: Some(1),
            removed: false,
        }],
    }
}

fn provider(id: u8, response: Result<Option<EvmObservedBlock>, ()>) -> Provider {
    Provider { id, response }
}

fn observe(
    quorum: &EvmBlockObservationQuorum<Provider>,
) -> Result<Option<EvmObservedBlock>, EvmBlockObservationQuorumError> {
    quorum.observed_block(EvmChainId::new(1).unwrap(), EvmAddress::new([7; 20]), 42)
}

#[test]
fn agreeing_independent_providers_return_the_complete_block() {
    let expected = block(9);
    let quorum = EvmBlockObservationQuorum::new(
        vec![
            provider(1, Ok(Some(expected.clone()))),
            provider(2, Ok(Some(expected.clone()))),
            provider(3, Err(())),
        ],
        2,
    )
    .unwrap();

    assert_eq!(observe(&quorum).unwrap(), Some(expected));
}

#[test]
fn insufficient_available_providers_return_none() {
    let quorum = EvmBlockObservationQuorum::new(
        vec![provider(1, Ok(Some(block(9)))), provider(2, Err(()))],
        2,
    )
    .unwrap();

    assert_eq!(observe(&quorum), Ok(None));
}

#[test]
fn any_successful_disagreement_fails_closed() {
    let quorum = EvmBlockObservationQuorum::new(
        vec![
            provider(1, Ok(Some(block(9)))),
            provider(2, Ok(Some(block(9)))),
            provider(3, Ok(Some(block(6)))),
        ],
        2,
    )
    .unwrap();

    assert_eq!(
        observe(&quorum),
        Err(EvmBlockObservationQuorumError::ProviderDisagreement)
    );
}

#[test]
fn malformed_provider_metadata_fails_closed() {
    let mut malformed = block(9);
    malformed.logs[0].block_number = Some(41);
    let quorum = EvmBlockObservationQuorum::new(
        vec![
            provider(1, Ok(Some(malformed))),
            provider(2, Ok(Some(block(9)))),
        ],
        2,
    )
    .unwrap();

    assert_eq!(
        observe(&quorum),
        Err(EvmBlockObservationQuorumError::ProviderResponseMismatch)
    );
}

#[test]
fn duplicate_provider_identity_is_rejected() {
    assert!(matches!(
        EvmBlockObservationQuorum::new(vec![provider(1, Ok(None)), provider(1, Ok(None))], 2,),
        Err(EvmBlockObservationQuorumError::DuplicateProvider)
    ));
}
