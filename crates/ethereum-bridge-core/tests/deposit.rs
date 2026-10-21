use ethereum_bridge_core::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmChainId, EvmDepositError,
    EvmDepositVerificationError, EvmFinalitySource, EvmFinalizedBlock, EvmHash, EvmLog,
    origin_asset_id, origin_network_id, verify_finalized_deposit,
};
use external_asset_core::{
    ConnectorStatus, ExternalAssetConfig, ExternalAssetCoordinator, ExternalAssetError,
};
use ledger_core::{AccountId, AssetId};

const PAYRAIL_ASSET: AssetId = AssetId::new([1; 32]);
const REGISTRY: AccountId = AccountId::new([2; 32]);
const OBSERVER: AccountId = AccountId::new([3; 32]);
const FINALITY: AccountId = AccountId::new([4; 32]);
const SETTLEMENT: AccountId = AccountId::new([5; 32]);
const RELEASE: AccountId = AccountId::new([6; 32]);
const RECIPIENT: AccountId = AccountId::new([7; 32]);
const BRIDGE: EvmAddress = EvmAddress::new([8; 20]);
const TOKEN: EvmAddress = EvmAddress::new([9; 20]);
const TRANSACTION: EvmHash = EvmHash::new([10; 32]);
const BLOCK: EvmHash = EvmHash::new([11; 32]);
const BLOCK_NUMBER: u64 = 1_234_567;

fn chain_id() -> EvmChainId {
    EvmChainId::new(11_155_111).unwrap()
}

fn binding(asset: EvmAsset) -> EvmAssetBinding {
    EvmAssetBinding::new(chain_id(), BRIDGE, asset, PAYRAIL_ASSET).unwrap()
}

fn asset_topic(asset: EvmAsset) -> EvmHash {
    let mut topic = [0_u8; 32];
    if let EvmAsset::Erc20(address) = asset {
        topic[12..].copy_from_slice(address.as_bytes());
    }
    EvmHash::new(topic)
}

fn data(amount: u128, nonce: u128) -> Vec<u8> {
    let mut encoded = vec![0_u8; 64];
    encoded[16..32].copy_from_slice(&amount.to_be_bytes());
    encoded[48..64].copy_from_slice(&nonce.to_be_bytes());
    encoded
}

fn log(asset: EvmAsset, amount: u128) -> EvmLog {
    EvmLog {
        address: BRIDGE,
        topics: vec![
            DEPOSITED_EVENT_TOPIC,
            EvmHash::new(*RECIPIENT.as_bytes()),
            asset_topic(asset),
        ],
        data: data(amount, 42),
        transaction_hash: Some(TRANSACTION),
        block_hash: Some(BLOCK),
        block_number: Some(BLOCK_NUMBER),
        log_index: Some(17),
        removed: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceError {
    Unavailable,
}

struct FinalityFixture {
    result: Result<Option<EvmFinalizedBlock>, SourceError>,
}

impl EvmFinalitySource for FinalityFixture {
    type Error = SourceError;

    fn finalized_canonical_block(
        &self,
        requested_chain: EvmChainId,
        requested_number: u64,
    ) -> Result<Option<EvmFinalizedBlock>, Self::Error> {
        assert_eq!(requested_chain, chain_id());
        assert_eq!(requested_number, BLOCK_NUMBER);
        self.result
    }
}

fn finalized(hash: EvmHash) -> FinalityFixture {
    FinalityFixture {
        result: Ok(Some(EvmFinalizedBlock {
            number: BLOCK_NUMBER,
            hash,
            proof_hash: [12; 32],
        })),
    }
}

#[test]
fn finalized_erc20_deposit_becomes_canonical_external_claim() {
    let verified = verify_finalized_deposit(
        binding(EvmAsset::Erc20(TOKEN)),
        &log(EvmAsset::Erc20(TOKEN), 500),
        &finalized(BLOCK),
    )
    .unwrap();

    assert_eq!(verified.claim().asset, PAYRAIL_ASSET);
    assert_eq!(
        verified.claim().origin_network,
        origin_network_id(chain_id())
    );
    assert_eq!(
        verified.claim().origin_asset,
        origin_asset_id(chain_id(), EvmAsset::Erc20(TOKEN))
    );
    assert_eq!(
        verified.claim().transaction.as_bytes(),
        TRANSACTION.as_bytes()
    );
    assert_eq!(verified.claim().event_index, 17);
    assert_eq!(verified.claim().recipient, RECIPIENT);
    assert_eq!(verified.claim().amount, 500);
    assert_eq!(verified.evidence().block_height, BLOCK_NUMBER);
    assert_eq!(verified.evidence().block_hash, *BLOCK.as_bytes());
    assert_eq!(verified.evidence().proof_hash, [12; 32]);
}

#[test]
fn native_deposit_uses_a_distinct_origin_asset() {
    let verified = verify_finalized_deposit(
        binding(EvmAsset::Native),
        &log(EvmAsset::Native, 1),
        &finalized(BLOCK),
    )
    .unwrap();

    assert_eq!(
        verified.claim().origin_asset,
        origin_asset_id(chain_id(), EvmAsset::Native)
    );
    assert_ne!(
        verified.claim().origin_asset,
        origin_asset_id(chain_id(), EvmAsset::Erc20(TOKEN))
    );
}

#[test]
fn coordinator_makes_exact_retries_idempotent_and_rejects_changed_claims() {
    let asset = EvmAsset::Erc20(TOKEN);
    let verified =
        verify_finalized_deposit(binding(asset), &log(asset, 500), &finalized(BLOCK)).unwrap();
    let mut coordinator = ExternalAssetCoordinator::new(REGISTRY);
    coordinator
        .register_asset(
            REGISTRY,
            ExternalAssetConfig {
                asset: PAYRAIL_ASSET,
                origin_network: origin_network_id(chain_id()),
                origin_asset: origin_asset_id(chain_id(), asset),
                observer_authority: OBSERVER,
                finality_authority: FINALITY,
                settlement_authority: SETTLEMENT,
                release_authority: RELEASE,
                status: ConnectorStatus::Active,
            },
        )
        .unwrap();

    let deposit = coordinator
        .observe_deposit(OBSERVER, verified.claim())
        .unwrap();
    assert_eq!(
        coordinator.observe_deposit(OBSERVER, verified.claim()),
        Ok(deposit)
    );
    coordinator
        .finalize_deposit(FINALITY, deposit, verified.evidence())
        .unwrap();
    assert_eq!(
        coordinator.finalize_deposit(FINALITY, deposit, verified.evidence()),
        Ok(())
    );

    let changed =
        verify_finalized_deposit(binding(asset), &log(asset, 501), &finalized(BLOCK)).unwrap();
    assert_eq!(
        coordinator.observe_deposit(OBSERVER, changed.claim()),
        Err(ExternalAssetError::DepositReferenceConflict)
    );
}

#[test]
fn wrong_contract_event_asset_and_removed_log_are_rejected() {
    let asset = EvmAsset::Erc20(TOKEN);
    let expected = binding(asset);

    let mut wrong_contract = log(asset, 1);
    wrong_contract.address = EvmAddress::new([99; 20]);
    assert_eq!(
        verify_finalized_deposit(expected, &wrong_contract, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::WrongBridgeContract
        ))
    );

    let mut wrong_event = log(asset, 1);
    wrong_event.topics[0] = EvmHash::new([99; 32]);
    assert_eq!(
        verify_finalized_deposit(expected, &wrong_event, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::WrongEventSignature
        ))
    );

    assert_eq!(
        verify_finalized_deposit(expected, &log(EvmAsset::Native, 1), &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::WrongOriginAsset
        ))
    );

    let mut removed = log(asset, 1);
    removed.removed = true;
    assert_eq!(
        verify_finalized_deposit(expected, &removed, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::RemovedLog
        ))
    );
}

#[test]
fn malformed_and_out_of_range_values_fail_closed() {
    let asset = EvmAsset::Erc20(TOKEN);
    let expected = binding(asset);

    let mut overflow = log(asset, 1);
    overflow.data[0] = 1;
    assert_eq!(
        verify_finalized_deposit(expected, &overflow, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::AmountOverflow
        ))
    );

    assert_eq!(
        verify_finalized_deposit(expected, &log(asset, 0), &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidAmount
        ))
    );

    let mut zero_recipient = log(asset, 1);
    zero_recipient.topics[1] = EvmHash::default();
    assert_eq!(
        verify_finalized_deposit(expected, &zero_recipient, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidRecipient
        ))
    );

    let mut padded_token = log(asset, 1);
    let mut topic = *padded_token.topics[2].as_bytes();
    topic[0] = 1;
    padded_token.topics[2] = EvmHash::new(topic);
    assert_eq!(
        verify_finalized_deposit(expected, &padded_token, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidTokenTopic
        ))
    );

    let mut index_overflow = log(asset, 1);
    index_overflow.log_index = Some(u64::from(u32::MAX) + 1);
    assert_eq!(
        verify_finalized_deposit(expected, &index_overflow, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::LogIndexOverflow
        ))
    );

    let mut truncated = log(asset, 1);
    truncated.data.pop();
    assert_eq!(
        verify_finalized_deposit(expected, &truncated, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidDataLength
        ))
    );

    let mut extra_topic = log(asset, 1);
    extra_topic.topics.push(EvmHash::default());
    assert_eq!(
        verify_finalized_deposit(expected, &extra_topic, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::WrongTopicCount
        ))
    );
}

#[test]
fn incomplete_receipt_metadata_is_rejected() {
    let asset = EvmAsset::Erc20(TOKEN);
    let expected = binding(asset);
    let mut incomplete = log(asset, 1);
    incomplete.transaction_hash = None;
    assert_eq!(
        verify_finalized_deposit(expected, &incomplete, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::MissingTransactionHash
        ))
    );

    let mut invalid = log(asset, 1);
    invalid.transaction_hash = Some(EvmHash::new([0; 32]));
    assert_eq!(
        verify_finalized_deposit(expected, &invalid, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidTransactionHash
        ))
    );

    let mut incomplete = log(asset, 1);
    incomplete.block_hash = None;
    assert_eq!(
        verify_finalized_deposit(expected, &incomplete, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::MissingBlockHash
        ))
    );

    let mut invalid = log(asset, 1);
    invalid.block_hash = Some(EvmHash::new([0; 32]));
    assert_eq!(
        verify_finalized_deposit(expected, &invalid, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::InvalidBlockHash
        ))
    );

    let mut incomplete = log(asset, 1);
    incomplete.block_number = None;
    assert_eq!(
        verify_finalized_deposit(expected, &incomplete, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::MissingBlockNumber
        ))
    );

    let mut incomplete = log(asset, 1);
    incomplete.log_index = None;
    assert_eq!(
        verify_finalized_deposit(expected, &incomplete, &finalized(BLOCK)),
        Err(EvmDepositVerificationError::Deposit(
            EvmDepositError::MissingLogIndex
        ))
    );
}

#[test]
fn finality_and_canonicality_failures_never_return_a_claim() {
    let asset = EvmAsset::Erc20(TOKEN);
    let expected = binding(asset);
    let receipt = log(asset, 1);

    let unavailable = FinalityFixture {
        result: Err(SourceError::Unavailable),
    };
    assert_eq!(
        verify_finalized_deposit(expected, &receipt, &unavailable),
        Err(EvmDepositVerificationError::Source(
            SourceError::Unavailable
        ))
    );

    let pending = FinalityFixture { result: Ok(None) };
    assert_eq!(
        verify_finalized_deposit(expected, &receipt, &pending),
        Err(EvmDepositVerificationError::BlockNotFinalized)
    );

    assert_eq!(
        verify_finalized_deposit(expected, &receipt, &finalized(EvmHash::new([88; 32]))),
        Err(EvmDepositVerificationError::NonCanonicalBlock)
    );

    let wrong_height = FinalityFixture {
        result: Ok(Some(EvmFinalizedBlock {
            number: BLOCK_NUMBER + 1,
            hash: BLOCK,
            proof_hash: [12; 32],
        })),
    };
    assert_eq!(
        verify_finalized_deposit(expected, &receipt, &wrong_height),
        Err(EvmDepositVerificationError::FinalizedBlockNumberMismatch)
    );

    let missing_proof = FinalityFixture {
        result: Ok(Some(EvmFinalizedBlock {
            number: BLOCK_NUMBER,
            hash: BLOCK,
            proof_hash: [0; 32],
        })),
    };
    assert_eq!(
        verify_finalized_deposit(expected, &receipt, &missing_proof),
        Err(EvmDepositVerificationError::MissingFinalityProof)
    );
}

#[test]
fn invalid_bindings_are_rejected_before_processing_logs() {
    assert_eq!(
        EvmAssetBinding::new(
            chain_id(),
            EvmAddress::default(),
            EvmAsset::Native,
            PAYRAIL_ASSET
        ),
        Err(EvmDepositError::ZeroBridgeContract)
    );
    assert_eq!(
        EvmAssetBinding::new(
            chain_id(),
            BRIDGE,
            EvmAsset::Erc20(EvmAddress::default()),
            PAYRAIL_ASSET
        ),
        Err(EvmDepositError::ZeroTokenContract)
    );
    assert_eq!(EvmChainId::new(0), None);
}
