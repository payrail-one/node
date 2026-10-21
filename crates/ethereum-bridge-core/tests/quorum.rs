use ethereum_bridge_core::{
    EvmChainId, EvmFinalityProvider, EvmFinalityQuorum, EvmFinalityQuorumError, EvmFinalitySource,
    EvmHash, EvmProviderFinality, EvmProviderId, MAX_FINALITY_PROVIDERS,
};

const BLOCK_NUMBER: u64 = 100;
const BLOCK_HASH: EvmHash = EvmHash::new([10; 32]);

fn chain_id() -> EvmChainId {
    EvmChainId::new(1).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Response {
    View(EvmProviderFinality),
    Pending,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
struct Provider {
    id: EvmProviderId,
    response: Response,
}

impl EvmFinalityProvider for Provider {
    type Error = ();

    fn provider_id(&self) -> EvmProviderId {
        self.id
    }

    fn finalized_view(
        &self,
        requested_chain: EvmChainId,
        requested_block: u64,
    ) -> Result<Option<EvmProviderFinality>, Self::Error> {
        assert_eq!(requested_chain, chain_id());
        assert_eq!(requested_block, BLOCK_NUMBER);
        match self.response {
            Response::View(view) => Ok(Some(view)),
            Response::Pending => Ok(None),
            Response::Unavailable => Err(()),
        }
    }
}

fn view(block_hash: EvmHash, finalized_height: u64, finalized_hash: u8) -> EvmProviderFinality {
    EvmProviderFinality {
        block_number: BLOCK_NUMBER,
        block_hash,
        finalized_head_number: finalized_height,
        finalized_head_hash: EvmHash::new([finalized_hash; 32]),
    }
}

fn provider(id: u8, response: Response) -> Provider {
    Provider {
        id: EvmProviderId::new([id; 32]),
        response,
    }
}

#[test]
fn two_independent_agreeing_views_establish_finality() {
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::View(view(BLOCK_HASH, 121, 22))),
        ],
        2,
    )
    .unwrap();

    let finalized = quorum
        .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
        .unwrap()
        .unwrap();
    assert_eq!(finalized.number, BLOCK_NUMBER);
    assert_eq!(finalized.hash, BLOCK_HASH);
    assert_ne!(finalized.proof_hash, [0; 32]);
}

#[test]
fn evidence_commitment_is_independent_of_provider_configuration_order() {
    let first = provider(1, Response::View(view(BLOCK_HASH, 120, 21)));
    let second = provider(2, Response::View(view(BLOCK_HASH, 121, 22)));
    let forward = EvmFinalityQuorum::new(vec![first, second], 2)
        .unwrap()
        .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
        .unwrap()
        .unwrap();
    let reverse = EvmFinalityQuorum::new(vec![second, first], 2)
        .unwrap()
        .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
        .unwrap()
        .unwrap();

    assert_eq!(forward, reverse);
}

#[test]
fn one_unavailable_provider_is_tolerated_only_when_quorum_remains() {
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::Unavailable),
            provider(3, Response::View(view(BLOCK_HASH, 122, 23))),
        ],
        2,
    )
    .unwrap();
    assert!(
        quorum
            .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
            .unwrap()
            .is_some()
    );

    let no_quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::Unavailable),
            provider(3, Response::Pending),
        ],
        2,
    )
    .unwrap();
    assert_eq!(
        no_quorum.finalized_canonical_block(chain_id(), BLOCK_NUMBER),
        Ok(None)
    );
}

#[test]
fn evidence_is_stable_when_the_same_policy_has_a_different_available_quorum() {
    let all_available = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::View(view(BLOCK_HASH, 121, 22))),
            provider(3, Response::View(view(BLOCK_HASH, 122, 23))),
        ],
        2,
    )
    .unwrap()
    .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
    .unwrap()
    .unwrap();
    let one_unavailable = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 130, 31))),
            provider(2, Response::Unavailable),
            provider(3, Response::View(view(BLOCK_HASH, 132, 33))),
        ],
        2,
    )
    .unwrap()
    .finalized_canonical_block(chain_id(), BLOCK_NUMBER)
    .unwrap()
    .unwrap();

    assert_eq!(all_available, one_unavailable);
}

#[test]
fn any_canonical_block_disagreement_fails_closed() {
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::View(view(EvmHash::new([99; 32]), 120, 21))),
            provider(3, Response::View(view(BLOCK_HASH, 120, 21))),
        ],
        2,
    )
    .unwrap();

    assert_eq!(
        quorum.finalized_canonical_block(chain_id(), BLOCK_NUMBER),
        Err(EvmFinalityQuorumError::ProviderDisagreement)
    );
}

#[test]
fn providers_cannot_report_different_hashes_for_the_same_finalized_height() {
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(view(BLOCK_HASH, 120, 21))),
            provider(2, Response::View(view(BLOCK_HASH, 120, 22))),
        ],
        2,
    )
    .unwrap();

    assert_eq!(
        quorum.finalized_canonical_block(chain_id(), BLOCK_NUMBER),
        Err(EvmFinalityQuorumError::ProviderDisagreement)
    );
}

#[test]
fn malformed_provider_view_fails_instead_of_being_counted() {
    let malformed_number = EvmProviderFinality {
        block_number: BLOCK_NUMBER + 1,
        ..view(BLOCK_HASH, 120, 21)
    };
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(malformed_number)),
            provider(2, Response::View(view(BLOCK_HASH, 120, 21))),
        ],
        2,
    )
    .unwrap();
    assert_eq!(
        quorum.finalized_canonical_block(chain_id(), BLOCK_NUMBER),
        Err(EvmFinalityQuorumError::ProviderResponseMismatch)
    );

    let head_behind = view(BLOCK_HASH, BLOCK_NUMBER - 1, 21);
    let quorum = EvmFinalityQuorum::new(
        vec![
            provider(1, Response::View(head_behind)),
            provider(2, Response::View(view(BLOCK_HASH, 120, 21))),
        ],
        2,
    )
    .unwrap();
    assert_eq!(
        quorum.finalized_canonical_block(chain_id(), BLOCK_NUMBER),
        Err(EvmFinalityQuorumError::ProviderResponseMismatch)
    );
}

#[test]
fn unsafe_quorum_configuration_is_rejected() {
    let first = provider(1, Response::Pending);
    let duplicate = provider(1, Response::Unavailable);
    assert!(matches!(
        EvmFinalityQuorum::new(vec![first], 1),
        Err(EvmFinalityQuorumError::InvalidProviderCount)
    ));
    assert!(matches!(
        EvmFinalityQuorum::new(vec![first, provider(2, Response::Pending)], 1),
        Err(EvmFinalityQuorumError::InvalidThreshold)
    ));
    assert!(matches!(
        EvmFinalityQuorum::new(vec![first, duplicate], 2),
        Err(EvmFinalityQuorumError::DuplicateProvider)
    ));
    let too_many = (0..=MAX_FINALITY_PROVIDERS)
        .map(|index| provider(u8::try_from(index).unwrap() + 1, Response::Pending))
        .collect();
    assert!(matches!(
        EvmFinalityQuorum::new(too_many, 2),
        Err(EvmFinalityQuorumError::InvalidProviderCount)
    ));
}
