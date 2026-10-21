use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmChainId, EvmFinalitySource,
    EvmFinalizedBlock, EvmHash, EvmLog, EvmObserverError, EvmObserverScope,
    FinalizedEvmObserverBlock, MAX_DEPOSITS_PER_BLOCK, prepare_observer_block,
    verify_finalized_deposit,
};
use ledger_core::{AccountId, AssetId};

const BRIDGE: EvmAddress = EvmAddress::new([1; 20]);
const TOKEN: EvmAddress = EvmAddress::new([2; 20]);
const PARENT: EvmHash = EvmHash::new([3; 32]);
const BLOCK: EvmHash = EvmHash::new([4; 32]);
const ASSET: AssetId = AssetId::new([5; 32]);
const RECIPIENT: AccountId = AccountId::new([6; 32]);

fn chain_id(value: u64) -> EvmChainId {
    EvmChainId::new(value).unwrap()
}

fn scope(first_block: u64) -> EvmObserverScope {
    EvmObserverScope::new(chain_id(1), BRIDGE, first_block, PARENT).unwrap()
}

fn binding(chain: EvmChainId) -> EvmAssetBinding {
    EvmAssetBinding::new(chain, BRIDGE, EvmAsset::Erc20(TOKEN), ASSET).unwrap()
}

fn finalized(number: u64, hash: EvmHash) -> EvmFinalizedBlock {
    EvmFinalizedBlock {
        number,
        hash,
        proof_hash: [7; 32],
    }
}

struct Source(EvmFinalizedBlock);

impl EvmFinalitySource for Source {
    type Error = ();

    fn finalized_canonical_block(
        &self,
        _chain_id: EvmChainId,
        _number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error> {
        Ok(Some(self.0))
    }
}

fn log(block_number: u64, block_hash: EvmHash, log_index: u64, marker: u8) -> EvmLog {
    let mut token_topic = [0_u8; 32];
    token_topic[12..].copy_from_slice(TOKEN.as_bytes());
    let mut data = vec![0_u8; 64];
    data[31] = marker;
    data[63] = marker;
    EvmLog {
        address: BRIDGE,
        topics: vec![
            DEPOSITED_EVENT_TOPIC,
            EvmHash::new(*RECIPIENT.as_bytes()),
            EvmHash::new(token_topic),
        ],
        data,
        transaction_hash: Some(EvmHash::new([marker; 32])),
        block_hash: Some(block_hash),
        block_number: Some(block_number),
        log_index: Some(log_index),
        removed: false,
    }
}

fn deposit(
    chain: EvmChainId,
    number: u64,
    hash: EvmHash,
    index: u64,
    marker: u8,
) -> ethereum_bridge_core::VerifiedEvmDeposit {
    verify_finalized_deposit(
        binding(chain),
        &log(number, hash, index, marker),
        &Source(finalized(number, hash)),
    )
    .unwrap()
}

#[test]
fn contiguous_finalized_block_prepares_cursor_and_ordered_deposits() {
    let observer_scope = scope(100);
    let prepared = prepare_observer_block(
        observer_scope,
        observer_scope.initial_checkpoint(),
        FinalizedEvmObserverBlock {
            finalized: finalized(100, BLOCK),
            parent_hash: PARENT,
            deposits: vec![
                deposit(chain_id(1), 100, BLOCK, 0, 10),
                deposit(chain_id(1), 100, BLOCK, 1, 11),
            ],
        },
    )
    .unwrap();

    assert_eq!(prepared.previous(), observer_scope.initial_checkpoint());
    assert_eq!(prepared.next().next_block, 101);
    assert_eq!(prepared.next().parent_hash, BLOCK);
    assert_eq!(prepared.deposits().len(), 2);
}

#[test]
fn empty_finalized_blocks_advance_without_inventing_deposits() {
    let observer_scope = scope(100);
    let prepared = prepare_observer_block(
        observer_scope,
        observer_scope.initial_checkpoint(),
        FinalizedEvmObserverBlock {
            finalized: finalized(100, BLOCK),
            parent_hash: PARENT,
            deposits: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(prepared.next().next_block, 101);
    assert!(prepared.deposits().is_empty());
}

#[test]
fn gaps_forks_and_missing_evidence_fail_before_cursor_advance() {
    let observer_scope = scope(100);
    let checkpoint = observer_scope.initial_checkpoint();
    let base = FinalizedEvmObserverBlock {
        finalized: finalized(100, BLOCK),
        parent_hash: PARENT,
        deposits: Vec::new(),
    };

    let mut gap = base.clone();
    gap.finalized.number = 101;
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, gap),
        Err(EvmObserverError::BlockSequenceMismatch)
    );

    let mut fork = base.clone();
    fork.parent_hash = EvmHash::new([99; 32]);
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, fork),
        Err(EvmObserverError::ParentHashMismatch)
    );

    let mut no_proof = base;
    no_proof.finalized.proof_hash = [0; 32];
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, no_proof),
        Err(EvmObserverError::MissingFinalityProof)
    );
}

#[test]
fn deposits_must_match_network_block_evidence_and_log_order() {
    let observer_scope = scope(100);
    let checkpoint = observer_scope.initial_checkpoint();

    let wrong_network = FinalizedEvmObserverBlock {
        finalized: finalized(100, BLOCK),
        parent_hash: PARENT,
        deposits: vec![deposit(chain_id(2), 100, BLOCK, 0, 10)],
    };
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, wrong_network),
        Err(EvmObserverError::WrongDepositNetwork)
    );

    let wrong_block = FinalizedEvmObserverBlock {
        finalized: finalized(100, BLOCK),
        parent_hash: PARENT,
        deposits: vec![deposit(chain_id(1), 101, EvmHash::new([8; 32]), 0, 10)],
    };
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, wrong_block),
        Err(EvmObserverError::DepositBlockMismatch)
    );

    let wrong_order = FinalizedEvmObserverBlock {
        finalized: finalized(100, BLOCK),
        parent_hash: PARENT,
        deposits: vec![
            deposit(chain_id(1), 100, BLOCK, 2, 10),
            deposit(chain_id(1), 100, BLOCK, 1, 11),
        ],
    };
    assert_eq!(
        prepare_observer_block(observer_scope, checkpoint, wrong_order),
        Err(EvmObserverError::DepositOrderMismatch)
    );
}

#[test]
fn batch_and_height_boundaries_fail_atomically() {
    let observer_scope = scope(100);
    let repeated = deposit(chain_id(1), 100, BLOCK, 0, 10);
    let oversized = FinalizedEvmObserverBlock {
        finalized: finalized(100, BLOCK),
        parent_hash: PARENT,
        deposits: vec![repeated; MAX_DEPOSITS_PER_BLOCK + 1],
    };
    assert_eq!(
        prepare_observer_block(
            observer_scope,
            observer_scope.initial_checkpoint(),
            oversized
        ),
        Err(EvmObserverError::TooManyDeposits)
    );

    let maximum_scope = scope(u64::MAX);
    assert_eq!(
        prepare_observer_block(
            maximum_scope,
            maximum_scope.initial_checkpoint(),
            FinalizedEvmObserverBlock {
                finalized: finalized(u64::MAX, BLOCK),
                parent_hash: PARENT,
                deposits: Vec::new(),
            }
        ),
        Err(EvmObserverError::HeightOverflow)
    );
}

#[test]
fn invalid_scope_anchor_is_rejected() {
    assert_eq!(
        EvmObserverScope::new(chain_id(1), EvmAddress::default(), 100, PARENT),
        Err(EvmObserverError::InvalidBridgeContract)
    );
    assert_eq!(
        EvmObserverScope::new(chain_id(1), BRIDGE, 100, EvmHash::default()),
        Err(EvmObserverError::InvalidParentHash)
    );
}
