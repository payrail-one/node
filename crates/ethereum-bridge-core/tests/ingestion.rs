use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmBlockObservationSource,
    EvmChainId, EvmFinalitySource, EvmFinalizedBlock, EvmHash, EvmLog, EvmObservedBlock,
    EvmObserverIngestionError, EvmObserverScope, ingest_next_observer_block,
};
use ledger_core::{AccountId, AssetId};

const BRIDGE: EvmAddress = EvmAddress::new([1; 20]);
const TOKEN: EvmAddress = EvmAddress::new([2; 20]);
const BLOCK: EvmHash = EvmHash::new([3; 32]);
const PARENT: EvmHash = EvmHash::new([4; 32]);
const HEIGHT: u64 = 42;

fn chain() -> EvmChainId {
    EvmChainId::new(1).unwrap()
}

fn scope() -> EvmObserverScope {
    EvmObserverScope::new(chain(), BRIDGE, HEIGHT, PARENT).unwrap()
}

fn binding(asset: EvmAsset, id: u8) -> EvmAssetBinding {
    EvmAssetBinding::new(chain(), BRIDGE, asset, AssetId::new([id; 32])).unwrap()
}

fn asset_topic(asset: EvmAsset) -> EvmHash {
    let mut bytes = [0_u8; 32];
    if let EvmAsset::Erc20(address) = asset {
        bytes[12..].copy_from_slice(address.as_bytes());
    }
    EvmHash::new(bytes)
}

fn log(asset: EvmAsset, index: u64) -> EvmLog {
    let mut data = vec![0_u8; 64];
    data[31] = 5;
    data[63] = u8::try_from(index).unwrap();
    EvmLog {
        address: BRIDGE,
        topics: vec![
            DEPOSITED_EVENT_TOPIC,
            EvmHash::new(*AccountId::new([7; 32]).as_bytes()),
            asset_topic(asset),
        ],
        data,
        transaction_hash: Some(EvmHash::new([u8::try_from(index).unwrap(); 32])),
        block_hash: Some(BLOCK),
        block_number: Some(HEIGHT),
        log_index: Some(index),
        removed: false,
    }
}

struct Observation(Result<Option<EvmObservedBlock>, ()>);

impl EvmBlockObservationSource for Observation {
    type Error = ();

    fn observed_block(
        &self,
        _chain_id: EvmChainId,
        _bridge_contract: EvmAddress,
        _block_number: u64,
    ) -> Result<Option<EvmObservedBlock>, Self::Error> {
        self.0.clone()
    }
}

struct Finality(Result<Option<EvmFinalizedBlock>, ()>);

impl EvmFinalitySource for Finality {
    type Error = ();

    fn finalized_canonical_block(
        &self,
        _chain_id: EvmChainId,
        _number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error> {
        self.0
    }
}

fn observation(logs: Vec<EvmLog>) -> Observation {
    Observation(Ok(Some(EvmObservedBlock {
        number: HEIGHT,
        hash: BLOCK,
        parent_hash: PARENT,
        logs,
    })))
}

fn finality(hash: EvmHash) -> Finality {
    Finality(Ok(Some(EvmFinalizedBlock {
        number: HEIGHT,
        hash,
        proof_hash: [9; 32],
    })))
}

#[test]
fn complete_quorum_evidence_prepares_the_next_atomic_commit() {
    let scope = scope();
    let bindings = [
        binding(EvmAsset::Native, 10),
        binding(EvmAsset::Erc20(TOKEN), 11),
    ];
    let prepared = ingest_next_observer_block(
        scope,
        scope.initial_checkpoint(),
        &bindings,
        &observation(vec![
            log(EvmAsset::Native, 1),
            log(EvmAsset::Erc20(TOKEN), 2),
        ]),
        &finality(BLOCK),
    )
    .unwrap();

    assert_eq!(prepared.deposits().len(), 2);
    assert_eq!(prepared.deposits()[0].claim().asset, AssetId::new([10; 32]));
    assert_eq!(prepared.deposits()[1].claim().asset, AssetId::new([11; 32]));
    assert_eq!(prepared.next().next_block, HEIGHT + 1);
    assert_eq!(prepared.next().parent_hash, BLOCK);
}

#[test]
fn observation_and_finality_must_name_the_same_exact_block() {
    let scope = scope();
    assert_eq!(
        ingest_next_observer_block(
            scope,
            scope.initial_checkpoint(),
            &[binding(EvmAsset::Native, 10)],
            &observation(vec![]),
            &finality(EvmHash::new([8; 32])),
        ),
        Err(EvmObserverIngestionError::FinalizedBlockMismatch)
    );
}

#[test]
fn unknown_assets_and_ambiguous_bindings_fail_closed() {
    let scope = scope();
    assert_eq!(
        ingest_next_observer_block(
            scope,
            scope.initial_checkpoint(),
            &[binding(EvmAsset::Native, 10)],
            &observation(vec![log(EvmAsset::Erc20(TOKEN), 1)]),
            &finality(BLOCK),
        ),
        Err(EvmObserverIngestionError::UnknownAsset)
    );

    assert_eq!(
        ingest_next_observer_block(
            scope,
            scope.initial_checkpoint(),
            &[binding(EvmAsset::Native, 10), binding(EvmAsset::Native, 11),],
            &observation(vec![]),
            &finality(BLOCK),
        ),
        Err(EvmObserverIngestionError::DuplicateOriginAsset)
    );
}

#[test]
fn unavailable_evidence_never_returns_a_prepared_block() {
    let scope = scope();
    let bindings = [binding(EvmAsset::Native, 10)];
    assert_eq!(
        ingest_next_observer_block(
            scope,
            scope.initial_checkpoint(),
            &bindings,
            &Observation(Ok(None)),
            &finality(BLOCK),
        ),
        Err(EvmObserverIngestionError::ObservationUnavailable)
    );
    assert_eq!(
        ingest_next_observer_block(
            scope,
            scope.initial_checkpoint(),
            &bindings,
            &observation(vec![]),
            &Finality(Ok(None)),
        ),
        Err(EvmObserverIngestionError::BlockNotFinalized)
    );
}
