use ledger_core::{
    AccountId, IdempotencyKey, NetworkId, OperationId, OperationKind, OperationOutcome,
    OperationReceipt,
};
use receipt_index_core::ReceiptIndexBase;
use sha2::{Digest, Sha256};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

use crate::{IndexedReceipt, ReceiptIndexStoreError};

const CURSOR_MAGIC: [u8; 8] = *b"RICUR001";
const BLOCK_MAGIC: [u8; 8] = *b"RIBLK001";
const RECEIPT_MAGIC: [u8; 8] = *b"RIRCP001";
const CHECKPOINT_LENGTH: usize = 104;
const CHECKSUM_LENGTH: usize = 32;
const CURSOR_BODY_LENGTH: usize = 8 + 32 + CHECKPOINT_LENGTH * 2 + 8 + 8;
const CURSOR_LENGTH: usize = CURSOR_BODY_LENGTH + CHECKSUM_LENGTH;
const BLOCK_BODY_LENGTH: usize = 8 + CHECKPOINT_LENGTH * 2 + 8 + 4 + 32;
const BLOCK_LENGTH: usize = BLOCK_BODY_LENGTH + CHECKSUM_LENGTH;
const RECEIPT_BODY_LENGTH: usize = 8 + CHECKPOINT_LENGTH + 4 + 32 + 32 + 32 + 8 + 8 + 8;
const RECEIPT_LENGTH: usize = RECEIPT_BODY_LENGTH + CHECKSUM_LENGTH;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CursorRecord {
    pub base: ReceiptIndexBase,
    pub latest: FinalizedCheckpoint,
    pub next_operation_index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BlockRecord {
    pub previous: FinalizedCheckpoint,
    pub checkpoint: FinalizedCheckpoint,
    pub first_operation_index: u64,
    pub receipt_count: u32,
    pub receipts_digest: [u8; 32],
}

pub(crate) fn encode_cursor(record: CursorRecord) -> [u8; CURSOR_LENGTH] {
    let mut output = [0_u8; CURSOR_LENGTH];
    output[..8].copy_from_slice(&CURSOR_MAGIC);
    output[8..40].copy_from_slice(record.base.network.as_bytes());
    encode_checkpoint(&mut output[40..144], record.base.checkpoint);
    encode_checkpoint(&mut output[144..248], record.latest);
    output[248..256].copy_from_slice(&record.base.next_operation_index.to_be_bytes());
    output[256..264].copy_from_slice(&record.next_operation_index.to_be_bytes());
    append_checksum(&mut output, CURSOR_BODY_LENGTH);
    output
}

pub(crate) fn decode_cursor(input: &[u8]) -> Result<CursorRecord, ReceiptIndexStoreError> {
    if input.len() != CURSOR_LENGTH || input[..8] != CURSOR_MAGIC {
        return Err(ReceiptIndexStoreError::CorruptRecord);
    }
    verify_checksum(input, CURSOR_BODY_LENGTH)?;
    Ok(CursorRecord {
        base: ReceiptIndexBase {
            network: NetworkId::new(array(&input[8..40])?),
            checkpoint: decode_checkpoint(&input[40..144])?,
            next_operation_index: u64::from_be_bytes(array(&input[248..256])?),
        },
        latest: decode_checkpoint(&input[144..248])?,
        next_operation_index: u64::from_be_bytes(array(&input[256..264])?),
    })
}

pub(crate) fn encode_block(record: BlockRecord) -> [u8; BLOCK_LENGTH] {
    let mut output = [0_u8; BLOCK_LENGTH];
    output[..8].copy_from_slice(&BLOCK_MAGIC);
    encode_checkpoint(&mut output[8..112], record.previous);
    encode_checkpoint(&mut output[112..216], record.checkpoint);
    output[216..224].copy_from_slice(&record.first_operation_index.to_be_bytes());
    output[224..228].copy_from_slice(&record.receipt_count.to_be_bytes());
    output[228..260].copy_from_slice(&record.receipts_digest);
    append_checksum(&mut output, BLOCK_BODY_LENGTH);
    output
}

pub(crate) fn decode_block(input: &[u8]) -> Result<BlockRecord, ReceiptIndexStoreError> {
    if input.len() != BLOCK_LENGTH || input[..8] != BLOCK_MAGIC {
        return Err(ReceiptIndexStoreError::CorruptRecord);
    }
    verify_checksum(input, BLOCK_BODY_LENGTH)?;
    Ok(BlockRecord {
        previous: decode_checkpoint(&input[8..112])?,
        checkpoint: decode_checkpoint(&input[112..216])?,
        first_operation_index: u64::from_be_bytes(array(&input[216..224])?),
        receipt_count: u32::from_be_bytes(array(&input[224..228])?),
        receipts_digest: array(&input[228..260])?,
    })
}

pub(crate) fn encode_receipt(value: IndexedReceipt) -> [u8; RECEIPT_LENGTH] {
    let mut output = [0_u8; RECEIPT_LENGTH];
    output[..8].copy_from_slice(&RECEIPT_MAGIC);
    encode_checkpoint(&mut output[8..112], value.checkpoint);
    output[112..116].copy_from_slice(&value.operation_offset.to_be_bytes());
    output[116..148].copy_from_slice(value.receipt.operation_id.as_bytes());
    output[148..180].copy_from_slice(value.receipt.account.as_bytes());
    output[180..212].copy_from_slice(value.receipt.idempotency_key.as_bytes());
    output[212..220].copy_from_slice(&value.receipt.nonce.to_be_bytes());
    output[220..228].copy_from_slice(&value.receipt.operation_index.to_be_bytes());
    output[228] = operation_kind(value.receipt.kind);
    output[229] = operation_outcome(value.receipt.outcome);
    append_checksum(&mut output, RECEIPT_BODY_LENGTH);
    output
}

pub(crate) fn decode_receipt(input: &[u8]) -> Result<IndexedReceipt, ReceiptIndexStoreError> {
    if input.len() != RECEIPT_LENGTH || input[..8] != RECEIPT_MAGIC {
        return Err(ReceiptIndexStoreError::CorruptRecord);
    }
    verify_checksum(input, RECEIPT_BODY_LENGTH)?;
    if input[230..236].iter().any(|byte| *byte != 0) {
        return Err(ReceiptIndexStoreError::CorruptRecord);
    }
    Ok(IndexedReceipt {
        checkpoint: decode_checkpoint(&input[8..112])?,
        operation_offset: u32::from_be_bytes(array(&input[112..116])?),
        receipt: OperationReceipt {
            operation_id: OperationId::new(array(&input[116..148])?),
            account: AccountId::new(array(&input[148..180])?),
            idempotency_key: IdempotencyKey::new(array(&input[180..212])?),
            nonce: u64::from_be_bytes(array(&input[212..220])?),
            operation_index: u64::from_be_bytes(array(&input[220..228])?),
            kind: decode_operation_kind(input[228])?,
            outcome: decode_operation_outcome(input[229])?,
        },
    })
}

pub(crate) fn receipts_digest(receipts: &[IndexedReceipt]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"payment.receipt-index.block.v1\0");
    for receipt in receipts {
        digest.update(encode_receipt(*receipt));
    }
    digest.finalize().into()
}

fn operation_kind(kind: OperationKind) -> u8 {
    match kind {
        OperationKind::Transfer => 0,
        OperationKind::SponsoredTransfer => 1,
        OperationKind::BatchTransfer => 2,
        OperationKind::SponsoredBatchTransfer => 3,
    }
}

fn decode_operation_kind(value: u8) -> Result<OperationKind, ReceiptIndexStoreError> {
    match value {
        0 => Ok(OperationKind::Transfer),
        1 => Ok(OperationKind::SponsoredTransfer),
        2 => Ok(OperationKind::BatchTransfer),
        3 => Ok(OperationKind::SponsoredBatchTransfer),
        _ => Err(ReceiptIndexStoreError::CorruptRecord),
    }
}

const fn operation_outcome(outcome: OperationOutcome) -> u8 {
    match outcome {
        OperationOutcome::Applied => 0,
        OperationOutcome::Expired => 1,
    }
}

fn decode_operation_outcome(value: u8) -> Result<OperationOutcome, ReceiptIndexStoreError> {
    match value {
        0 => Ok(OperationOutcome::Applied),
        1 => Ok(OperationOutcome::Expired),
        _ => Err(ReceiptIndexStoreError::CorruptRecord),
    }
}

fn encode_checkpoint(output: &mut [u8], checkpoint: FinalizedCheckpoint) {
    output[..8].copy_from_slice(&checkpoint.height.to_be_bytes());
    output[8..40].copy_from_slice(checkpoint.block_hash.as_bytes());
    output[40..72].copy_from_slice(checkpoint.state_root.as_bytes());
    output[72..104].copy_from_slice(checkpoint.validator_set_hash.as_bytes());
}

fn decode_checkpoint(input: &[u8]) -> Result<FinalizedCheckpoint, ReceiptIndexStoreError> {
    if input.len() != CHECKPOINT_LENGTH {
        return Err(ReceiptIndexStoreError::CorruptRecord);
    }
    Ok(FinalizedCheckpoint {
        height: u64::from_be_bytes(array(&input[..8])?),
        block_hash: BlockHash::new(array(&input[8..40])?),
        state_root: StateRoot::new(array(&input[40..72])?),
        validator_set_hash: ValidatorSetHash::new(array(&input[72..104])?),
    })
}

fn append_checksum<const N: usize>(output: &mut [u8; N], body_length: usize) {
    let digest: [u8; 32] = Sha256::digest(&output[..body_length]).into();
    output[body_length..].copy_from_slice(&digest);
}

fn verify_checksum(input: &[u8], body_length: usize) -> Result<(), ReceiptIndexStoreError> {
    let expected: [u8; 32] = Sha256::digest(&input[..body_length]).into();
    if input[body_length..] == expected {
        Ok(())
    } else {
        Err(ReceiptIndexStoreError::CorruptRecord)
    }
}

fn array<const N: usize>(input: &[u8]) -> Result<[u8; N], ReceiptIndexStoreError> {
    input
        .try_into()
        .map_err(|_| ReceiptIndexStoreError::CorruptRecord)
}
