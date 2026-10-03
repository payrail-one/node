use ledger_core::{
    AccountId, IdempotencyKey, NetworkId, OperationId, OperationKind, OperationOutcome,
    OperationReceipt,
};
use payment_idempotency_core::{
    ClientRequestKey, FinalizedPayment, IndeterminateReason, PaymentIntent, PaymentRequestId,
    PaymentReservation, PreparedSubmission, RejectionCode, RequestDigest, ReservationState,
    TenantId,
};
use sha2::{Digest, Sha256};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};
use transaction_protocol::MAX_ENVELOPE_BYTES;

use crate::PaymentIdempotencyStoreError;

const RECORD_MAGIC: [u8; 8] = *b"PIDEM002";
const CHECKSUM_LENGTH: usize = 32;
const CHECKPOINT_LENGTH: usize = 104;

pub(crate) fn request_key(id: PaymentRequestId) -> [u8; 96] {
    let mut key = [0_u8; 96];
    key[..32].copy_from_slice(id.tenant.as_bytes());
    key[32..64].copy_from_slice(id.account.as_bytes());
    key[64..].copy_from_slice(id.client_key.as_bytes());
    key
}

pub(crate) fn decode_request_key(
    input: &[u8],
) -> Result<PaymentRequestId, PaymentIdempotencyStoreError> {
    if input.len() != 96 {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    Ok(PaymentRequestId {
        tenant: TenantId::new(array(&input[..32])?),
        account: AccountId::new(array(&input[32..64])?),
        client_key: ClientRequestKey::new(array(&input[64..])?),
    })
}

pub(crate) fn account_nonce_key(account: AccountId, nonce: u64) -> [u8; 40] {
    let mut key = [0_u8; 40];
    key[..32].copy_from_slice(account.as_bytes());
    key[32..].copy_from_slice(&nonce.to_be_bytes());
    key
}

pub(crate) fn encode_record(
    reservation: &PaymentReservation,
) -> Result<Vec<u8>, PaymentIdempotencyStoreError> {
    let intent = reservation.intent();
    let mut output = Vec::new();
    output.extend_from_slice(&RECORD_MAGIC);
    output.extend_from_slice(intent.network.as_bytes());
    output.extend_from_slice(intent.request_id.tenant.as_bytes());
    output.extend_from_slice(intent.request_id.account.as_bytes());
    output.extend_from_slice(intent.request_id.client_key.as_bytes());
    output.extend_from_slice(intent.request_digest.as_bytes());
    output.extend_from_slice(intent.ledger_idempotency_key.as_bytes());
    output.extend_from_slice(&reservation.created_at_ms().to_be_bytes());
    output.extend_from_slice(&reservation.updated_at_ms().to_be_bytes());
    encode_state(&mut output, reservation.state())?;
    let checksum: [u8; 32] = Sha256::digest(&output).into();
    output.extend_from_slice(&checksum);
    Ok(output)
}

pub(crate) fn decode_record(
    input: &[u8],
) -> Result<PaymentReservation, PaymentIdempotencyStoreError> {
    if input.len() < 8 + 32 * 6 + 8 * 2 + 1 + CHECKSUM_LENGTH {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    let body_length = input
        .len()
        .checked_sub(CHECKSUM_LENGTH)
        .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
    let expected: [u8; 32] = Sha256::digest(&input[..body_length]).into();
    if input[body_length..] != expected {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    let mut decoder = Decoder::new(&input[..body_length]);
    if decoder.array::<8>()? != RECORD_MAGIC {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    let intent = PaymentIntent {
        network: NetworkId::new(decoder.array()?),
        request_id: PaymentRequestId {
            tenant: TenantId::new(decoder.array()?),
            account: AccountId::new(decoder.array()?),
            client_key: ClientRequestKey::new(decoder.array()?),
        },
        request_digest: RequestDigest::new(decoder.array()?),
        ledger_idempotency_key: IdempotencyKey::new(decoder.array()?),
    };
    let created_at_ms = decoder.u64()?;
    let updated_at_ms = decoder.u64()?;
    let state = decode_state(&mut decoder)?;
    decoder.finish()?;
    PaymentReservation::restore(intent, created_at_ms, updated_at_ms, state).map_err(Into::into)
}

fn encode_state(
    output: &mut Vec<u8>,
    state: &ReservationState,
) -> Result<(), PaymentIdempotencyStoreError> {
    match state {
        ReservationState::Reserved => output.push(0),
        ReservationState::NonceAssigned(nonce) => {
            output.push(1);
            output.extend_from_slice(&nonce.to_be_bytes());
        }
        ReservationState::SubmissionPrepared(submission) => {
            output.push(2);
            encode_submission(output, submission)?;
        }
        ReservationState::Published(submission) => {
            output.push(6);
            encode_submission(output, submission)?;
        }
        ReservationState::Indeterminate { submission, reason } => {
            output.push(3);
            encode_submission(output, submission)?;
            output.push(indeterminate_reason(*reason));
        }
        ReservationState::Finalized(finalized) => {
            output.push(4);
            encode_submission(output, &finalized.submission)?;
            encode_checkpoint(output, finalized.checkpoint);
            encode_receipt(output, finalized.receipt);
        }
        ReservationState::Rejected { code } => {
            output.push(5);
            output.push(rejection_code(*code));
        }
    }
    Ok(())
}

fn decode_state(
    decoder: &mut Decoder<'_>,
) -> Result<ReservationState, PaymentIdempotencyStoreError> {
    match decoder.u8()? {
        0 => Ok(ReservationState::Reserved),
        1 => Ok(ReservationState::NonceAssigned(decoder.u64()?)),
        2 => Ok(ReservationState::SubmissionPrepared(decode_submission(
            decoder,
        )?)),
        3 => Ok(ReservationState::Indeterminate {
            submission: decode_submission(decoder)?,
            reason: decode_indeterminate_reason(decoder.u8()?)?,
        }),
        4 => Ok(ReservationState::Finalized(Box::new(FinalizedPayment {
            submission: decode_submission(decoder)?,
            checkpoint: decode_checkpoint(decoder)?,
            receipt: decode_receipt(decoder)?,
        }))),
        5 => Ok(ReservationState::Rejected {
            code: decode_rejection_code(decoder.u8()?)?,
        }),
        6 => Ok(ReservationState::Published(decode_submission(decoder)?)),
        _ => Err(PaymentIdempotencyStoreError::CorruptRecord),
    }
}

fn encode_submission(
    output: &mut Vec<u8>,
    submission: &PreparedSubmission,
) -> Result<(), PaymentIdempotencyStoreError> {
    let length = u32::try_from(submission.envelope.len())
        .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?;
    output.extend_from_slice(&submission.nonce.to_be_bytes());
    output.extend_from_slice(submission.operation_id.as_bytes());
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&submission.envelope);
    Ok(())
}

fn decode_submission(
    decoder: &mut Decoder<'_>,
) -> Result<PreparedSubmission, PaymentIdempotencyStoreError> {
    let nonce = decoder.u64()?;
    let operation_id = OperationId::new(decoder.array()?);
    let envelope_length =
        usize::try_from(decoder.u32()?).map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?;
    if envelope_length > MAX_ENVELOPE_BYTES {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    Ok(PreparedSubmission {
        nonce,
        operation_id,
        envelope: decoder.bytes(envelope_length)?.to_vec(),
    })
}

fn encode_checkpoint(output: &mut Vec<u8>, checkpoint: FinalizedCheckpoint) {
    output.extend_from_slice(&checkpoint.height.to_be_bytes());
    output.extend_from_slice(checkpoint.block_hash.as_bytes());
    output.extend_from_slice(checkpoint.state_root.as_bytes());
    output.extend_from_slice(checkpoint.validator_set_hash.as_bytes());
}

fn decode_checkpoint(
    decoder: &mut Decoder<'_>,
) -> Result<FinalizedCheckpoint, PaymentIdempotencyStoreError> {
    let bytes = decoder.bytes(CHECKPOINT_LENGTH)?;
    Ok(FinalizedCheckpoint {
        height: u64::from_be_bytes(array(&bytes[..8])?),
        block_hash: BlockHash::new(array(&bytes[8..40])?),
        state_root: StateRoot::new(array(&bytes[40..72])?),
        validator_set_hash: ValidatorSetHash::new(array(&bytes[72..104])?),
    })
}

fn encode_receipt(output: &mut Vec<u8>, receipt: OperationReceipt) {
    output.extend_from_slice(receipt.operation_id.as_bytes());
    output.extend_from_slice(receipt.account.as_bytes());
    output.extend_from_slice(receipt.idempotency_key.as_bytes());
    output.extend_from_slice(&receipt.nonce.to_be_bytes());
    output.extend_from_slice(&receipt.operation_index.to_be_bytes());
    output.push(operation_kind(receipt.kind));
    output.push(operation_outcome(receipt.outcome));
}

fn decode_receipt(
    decoder: &mut Decoder<'_>,
) -> Result<OperationReceipt, PaymentIdempotencyStoreError> {
    Ok(OperationReceipt {
        operation_id: OperationId::new(decoder.array()?),
        account: AccountId::new(decoder.array()?),
        idempotency_key: IdempotencyKey::new(decoder.array()?),
        nonce: decoder.u64()?,
        operation_index: decoder.u64()?,
        kind: decode_operation_kind(decoder.u8()?)?,
        outcome: decode_operation_outcome(decoder.u8()?)?,
    })
}

fn operation_kind(value: OperationKind) -> u8 {
    match value {
        OperationKind::Transfer => 0,
        OperationKind::SponsoredTransfer => 1,
        OperationKind::BatchTransfer => 2,
        OperationKind::SponsoredBatchTransfer => 3,
    }
}

fn decode_operation_kind(value: u8) -> Result<OperationKind, PaymentIdempotencyStoreError> {
    match value {
        0 => Ok(OperationKind::Transfer),
        1 => Ok(OperationKind::SponsoredTransfer),
        2 => Ok(OperationKind::BatchTransfer),
        3 => Ok(OperationKind::SponsoredBatchTransfer),
        _ => Err(PaymentIdempotencyStoreError::CorruptRecord),
    }
}

const fn operation_outcome(value: OperationOutcome) -> u8 {
    match value {
        OperationOutcome::Applied => 0,
        OperationOutcome::Expired => 1,
    }
}

fn decode_operation_outcome(value: u8) -> Result<OperationOutcome, PaymentIdempotencyStoreError> {
    match value {
        0 => Ok(OperationOutcome::Applied),
        1 => Ok(OperationOutcome::Expired),
        _ => Err(PaymentIdempotencyStoreError::CorruptRecord),
    }
}

fn indeterminate_reason(value: IndeterminateReason) -> u8 {
    match value {
        IndeterminateReason::BroadcastOutcomeUnknown => 0,
        IndeterminateReason::ReconciliationRequired => 1,
    }
}

fn decode_indeterminate_reason(
    value: u8,
) -> Result<IndeterminateReason, PaymentIdempotencyStoreError> {
    match value {
        0 => Ok(IndeterminateReason::BroadcastOutcomeUnknown),
        1 => Ok(IndeterminateReason::ReconciliationRequired),
        _ => Err(PaymentIdempotencyStoreError::CorruptRecord),
    }
}

fn rejection_code(value: RejectionCode) -> u8 {
    match value {
        RejectionCode::InvalidRequest => 0,
        RejectionCode::PolicyDenied => 1,
    }
}

fn decode_rejection_code(value: u8) -> Result<RejectionCode, PaymentIdempotencyStoreError> {
    match value {
        0 => Ok(RejectionCode::InvalidRequest),
        1 => Ok(RejectionCode::PolicyDenied),
        _ => Err(PaymentIdempotencyStoreError::CorruptRecord),
    }
}

struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], PaymentIdempotencyStoreError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
        let value = self
            .input
            .get(self.position..end)
            .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], PaymentIdempotencyStoreError> {
        array(self.bytes(N)?)
    }

    fn u8(&mut self) -> Result<u8, PaymentIdempotencyStoreError> {
        self.bytes(1)?
            .first()
            .copied()
            .ok_or(PaymentIdempotencyStoreError::CorruptRecord)
    }

    fn u32(&mut self) -> Result<u32, PaymentIdempotencyStoreError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, PaymentIdempotencyStoreError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn finish(self) -> Result<(), PaymentIdempotencyStoreError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(PaymentIdempotencyStoreError::CorruptRecord)
        }
    }
}

fn array<const N: usize>(input: &[u8]) -> Result<[u8; N], PaymentIdempotencyStoreError> {
    input
        .try_into()
        .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)
}
