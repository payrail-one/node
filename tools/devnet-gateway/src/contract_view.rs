use account_address::AddressCodec;
use ledger_core::{AuthorizedOperation, ContractId, Event, Ledger, OperationOutcome};
use sha2::{Digest, Sha256};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::{
        ContractExecutionView, ContractStateEntryView, ContractView, FinalizedTransactionView,
    },
    service::ASSET,
};

pub(crate) fn contract_view(
    addresses: &AddressCodec,
    ledger: &Ledger,
    encoded_id: &str,
    finalized_height: u64,
) -> Result<ContractView, DevnetError> {
    let bytes = decode_bounded_hex(encoded_id, 32)?;
    let id = ContractId::new(bytes.try_into().map_err(|_| DevnetError::InvalidHex)?);
    let contract = ledger.contract(id).ok_or(DevnetError::ContractNotFound)?;
    let code_hash: [u8; 32] = Sha256::digest(&contract.code).into();
    Ok(ContractView {
        id: encode_hex(id.as_bytes()),
        owner: addresses
            .encode(contract.owner)
            .map_err(|_| DevnetError::InternalInvariant)?,
        code: encode_hex(&contract.code),
        code_hash: encode_hex(&code_hash),
        balance: ledger.balance(ASSET, id.account()).to_string(),
        state: ledger
            .contract_state_entries(id)
            .into_iter()
            .map(|entry| ContractStateEntryView {
                key: encode_hex(&entry.key),
                value: entry.value.to_string(),
            })
            .collect(),
        finalized_height: finalized_height.to_string(),
    })
}

pub(crate) fn transaction_view(
    addresses: &AddressCodec,
    operation: &AuthorizedOperation,
    receipt: ledger_core::OperationReceipt,
    checkpoint: FinalizedCheckpoint,
) -> Result<FinalizedTransactionView, DevnetError> {
    let (from, to, amount, fee, kind) = match operation {
        AuthorizedOperation::Transfer(transfer)
        | AuthorizedOperation::SponsoredTransfer { transfer, .. } => (
            transfer.from,
            transfer.to,
            transfer.amount,
            transfer.fee,
            if matches!(operation, AuthorizedOperation::Transfer(_)) {
                "transfer"
            } else {
                "sponsoredTransfer"
            },
        ),
        AuthorizedOperation::TransferBatch(batch)
        | AuthorizedOperation::SponsoredBatchTransfer { batch, .. } => (
            batch.from,
            batch.from,
            batch.items.iter().try_fold(0_u128, |total, item| {
                total
                    .checked_add(item.amount)
                    .ok_or(DevnetError::InternalInvariant)
            })?,
            batch.fee,
            "batchTransfer",
        ),
        AuthorizedOperation::ContractDeploy(deploy) => (
            deploy.owner,
            ledger_core::contract_id(deploy).account(),
            0,
            deploy.fee,
            "contractDeploy",
        ),
        AuthorizedOperation::ContractCall(call) => (
            call.caller,
            call.contract.account(),
            call.attached_amount,
            call.fee,
            "contractCall",
        ),
    };
    Ok(FinalizedTransactionView {
        id: encode_hex(receipt.operation_id.as_bytes()),
        block_height: checkpoint.height.to_string(),
        operation_index: receipt.operation_index.to_string(),
        from: addresses
            .encode(from)
            .map_err(|_| DevnetError::InternalInvariant)?,
        to: addresses
            .encode(to)
            .map_err(|_| DevnetError::InternalInvariant)?,
        amount: amount.to_string(),
        fee: fee.to_string(),
        outcome: match receipt.outcome {
            OperationOutcome::Applied => "applied",
            OperationOutcome::Expired => "expired",
        },
        kind,
    })
}

pub(crate) fn contract_execution_view(
    operation: &AuthorizedOperation,
    events: &[Event],
) -> Option<ContractExecutionView> {
    let contract_id = match operation {
        AuthorizedOperation::ContractDeploy(deploy) => ledger_core::contract_id(deploy),
        AuthorizedOperation::ContractCall(call) => call.contract,
        _ => return None,
    };
    let mut entrypoint = None;
    let mut execution_units = None;
    let mut code_hash = None;
    let mut topics = Vec::new();
    for event in events {
        match event {
            Event::ContractDeployed {
                contract,
                code_hash: hash,
                ..
            } if *contract == contract_id => code_hash = Some(encode_hex(hash)),
            Event::ContractCalled {
                contract,
                entrypoint: name,
                execution_units: units,
                ..
            } if *contract == contract_id => {
                entrypoint = Some(name.clone());
                execution_units = Some(units.to_string());
            }
            Event::ContractEmitted {
                contract, topic, ..
            } if *contract == contract_id => topics.push(encode_hex(topic)),
            _ => {}
        }
    }
    Some(ContractExecutionView {
        contract_id: encode_hex(contract_id.as_bytes()),
        entrypoint,
        execution_units,
        code_hash,
        events: topics,
    })
}
