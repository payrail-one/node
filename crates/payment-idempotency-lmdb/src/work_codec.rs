use payment_idempotency_core::PaymentRequestId;

use crate::{
    PaymentIdempotencyStoreError,
    codec::{decode_request_key, request_key},
};

const SCHEDULE_LENGTH: usize = 12;
const DUE_KEY_LENGTH: usize = 104;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoredSchedule {
    pub consecutive_failures: u32,
    pub next_attempt_at_ms: u64,
}

impl StoredSchedule {
    pub const INITIAL: Self = Self {
        consecutive_failures: 0,
        next_attempt_at_ms: 0,
    };
}

pub(crate) fn encode_schedule(schedule: StoredSchedule) -> [u8; SCHEDULE_LENGTH] {
    let mut encoded = [0_u8; SCHEDULE_LENGTH];
    encoded[..4].copy_from_slice(&schedule.consecutive_failures.to_be_bytes());
    encoded[4..].copy_from_slice(&schedule.next_attempt_at_ms.to_be_bytes());
    encoded
}

pub(crate) fn decode_schedule(
    encoded: &[u8],
) -> Result<StoredSchedule, PaymentIdempotencyStoreError> {
    if encoded.len() != SCHEDULE_LENGTH {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    Ok(StoredSchedule {
        consecutive_failures: u32::from_be_bytes(
            encoded[..4]
                .try_into()
                .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
        ),
        next_attempt_at_ms: u64::from_be_bytes(
            encoded[4..]
                .try_into()
                .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
        ),
    })
}

pub(crate) fn due_key(
    next_attempt_at_ms: u64,
    request_id: PaymentRequestId,
) -> [u8; DUE_KEY_LENGTH] {
    let mut key = [0_u8; DUE_KEY_LENGTH];
    key[..8].copy_from_slice(&next_attempt_at_ms.to_be_bytes());
    key[8..].copy_from_slice(&request_key(request_id));
    key
}

pub(crate) fn decode_due_key(
    encoded: &[u8],
) -> Result<(u64, PaymentRequestId), PaymentIdempotencyStoreError> {
    if encoded.len() != DUE_KEY_LENGTH {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    let next_attempt_at_ms = u64::from_be_bytes(
        encoded[..8]
            .try_into()
            .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
    );
    Ok((next_attempt_at_ms, decode_request_key(&encoded[8..])?))
}

pub(crate) fn maximum_due_key(now_ms: u64) -> [u8; DUE_KEY_LENGTH] {
    let mut key = [u8::MAX; DUE_KEY_LENGTH];
    key[..8].copy_from_slice(&now_ms.to_be_bytes());
    key
}
