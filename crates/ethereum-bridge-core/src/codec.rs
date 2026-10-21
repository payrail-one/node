use external_asset_core::{OriginDeposit, OriginTransactionId};
use ledger_core::AccountId;

use crate::{
    DEPOSITED_EVENT_TOPIC, EvmAddress, EvmAsset, EvmAssetBinding, EvmDepositError, EvmLog,
    ObservedEvmDeposit, origin_asset_id, origin_network_id,
};

const TOPIC_COUNT: usize = 3;
const DEPOSIT_DATA_LENGTH: usize = 64;
const WORD_LENGTH: usize = 32;
const ADDRESS_PADDING_LENGTH: usize = 12;

/// Decodes one log emitted by the bound bridge contract into the canonical
/// external-asset claim. This function validates log shape and asset identity;
/// it does not assert Ethereum finality.
///
/// # Errors
///
/// Returns an error for removed, malformed, incomplete, wrong-contract or
/// wrong-asset logs, and for amounts that cannot be represented by Payrail.
pub fn decode_deposit(
    binding: EvmAssetBinding,
    log: &EvmLog,
) -> Result<ObservedEvmDeposit, EvmDepositError> {
    if log.removed {
        return Err(EvmDepositError::RemovedLog);
    }
    if log.address != binding.bridge_contract() {
        return Err(EvmDepositError::WrongBridgeContract);
    }
    if log.topics.len() != TOPIC_COUNT {
        return Err(EvmDepositError::WrongTopicCount);
    }
    if log.topics[0] != DEPOSITED_EVENT_TOPIC {
        return Err(EvmDepositError::WrongEventSignature);
    }
    if log.data.len() != DEPOSIT_DATA_LENGTH {
        return Err(EvmDepositError::InvalidDataLength);
    }

    let recipient = AccountId::new(*log.topics[1].as_bytes());
    if recipient.as_bytes().iter().all(|byte| *byte == 0) {
        return Err(EvmDepositError::InvalidRecipient);
    }
    let observed_asset = decode_asset_topic(log.topics[2])?;
    if observed_asset != binding.origin_asset() {
        return Err(EvmDepositError::WrongOriginAsset);
    }
    let amount = decode_u128_word(&log.data[..WORD_LENGTH])?;
    if amount == 0 {
        return Err(EvmDepositError::InvalidAmount);
    }
    let deposit_nonce = log.data[WORD_LENGTH..]
        .try_into()
        .map_err(|_| EvmDepositError::InvalidDataLength)?;
    let transaction = log
        .transaction_hash
        .ok_or(EvmDepositError::MissingTransactionHash)?;
    if transaction.is_zero() {
        return Err(EvmDepositError::InvalidTransactionHash);
    }
    let block_hash = log.block_hash.ok_or(EvmDepositError::MissingBlockHash)?;
    if block_hash.is_zero() {
        return Err(EvmDepositError::InvalidBlockHash);
    }
    let block_number = log
        .block_number
        .ok_or(EvmDepositError::MissingBlockNumber)?;
    let raw_log_index = log.log_index.ok_or(EvmDepositError::MissingLogIndex)?;
    let event_index =
        u32::try_from(raw_log_index).map_err(|_| EvmDepositError::LogIndexOverflow)?;

    Ok(ObservedEvmDeposit {
        claim: OriginDeposit {
            asset: binding.payrail_asset(),
            origin_network: origin_network_id(binding.chain_id()),
            origin_asset: origin_asset_id(binding.chain_id(), observed_asset),
            transaction: OriginTransactionId::new(*transaction.as_bytes()),
            event_index,
            recipient,
            amount,
        },
        block_hash,
        block_number,
        deposit_nonce,
    })
}

fn decode_asset_topic(topic: crate::EvmHash) -> Result<EvmAsset, EvmDepositError> {
    let bytes = topic.as_bytes();
    if bytes[..ADDRESS_PADDING_LENGTH]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(EvmDepositError::InvalidTokenTopic);
    }
    let address = EvmAddress::new(
        bytes[ADDRESS_PADDING_LENGTH..]
            .try_into()
            .map_err(|_| EvmDepositError::InvalidTokenTopic)?,
    );
    if address.is_zero() {
        Ok(EvmAsset::Native)
    } else {
        Ok(EvmAsset::Erc20(address))
    }
}

fn decode_u128_word(word: &[u8]) -> Result<u128, EvmDepositError> {
    if word.len() != WORD_LENGTH {
        return Err(EvmDepositError::InvalidDataLength);
    }
    if word[..16].iter().any(|byte| *byte != 0) {
        return Err(EvmDepositError::AmountOverflow);
    }
    let value = word[16..]
        .try_into()
        .map(u128::from_be_bytes)
        .map_err(|_| EvmDepositError::InvalidDataLength)?;
    Ok(value)
}
