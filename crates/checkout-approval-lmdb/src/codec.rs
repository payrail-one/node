use checkout_approval_core::{
    ApprovalCodeClaim, ApprovalCodeDigest, ApprovalCodeRecord, ApprovalCodeState,
};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;
use sha2::{Digest, Sha256};

use crate::ApprovalCodeStoreError;

const RECORD_MAGIC: [u8; 8] = *b"APRV0001";
const CHECKSUM_LENGTH: usize = 32;

pub(crate) fn digest_key(digest: ApprovalCodeDigest) -> [u8; 32] {
    *digest.as_bytes()
}

pub(crate) fn account_key(account: AccountId) -> [u8; 32] {
    *account.as_bytes()
}

pub(crate) fn checkout_key(checkout: CheckoutId) -> [u8; 32] {
    *checkout.as_bytes()
}

pub(crate) fn encode_record(record: ApprovalCodeRecord) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(177);
    encoded.extend_from_slice(&RECORD_MAGIC);
    encoded.extend_from_slice(record.network().as_bytes());
    encoded.extend_from_slice(record.digest().as_bytes());
    encoded.extend_from_slice(record.account().as_bytes());
    encoded.extend_from_slice(&record.issued_at_ms().to_be_bytes());
    encoded.extend_from_slice(&record.expires_at_ms().to_be_bytes());
    encode_state(&mut encoded, record.state());
    let checksum: [u8; 32] = Sha256::digest(&encoded).into();
    encoded.extend_from_slice(&checksum);
    encoded
}

pub(crate) fn decode_record(input: &[u8]) -> Result<ApprovalCodeRecord, ApprovalCodeStoreError> {
    let body_length = input
        .len()
        .checked_sub(CHECKSUM_LENGTH)
        .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
    let expected: [u8; 32] = Sha256::digest(&input[..body_length]).into();
    if input[body_length..] != expected {
        return Err(ApprovalCodeStoreError::CorruptRecord);
    }
    let mut decoder = Decoder::new(&input[..body_length]);
    if decoder.array::<8>()? != RECORD_MAGIC {
        return Err(ApprovalCodeStoreError::CorruptRecord);
    }
    let network = NetworkId::new(decoder.array()?);
    let digest = ApprovalCodeDigest::new(decoder.array()?);
    let account = AccountId::new(decoder.array()?);
    let issued_at_ms = decoder.u64()?;
    let expires_at_ms = decoder.u64()?;
    let state = decode_state(&mut decoder)?;
    decoder.finish()?;
    ApprovalCodeRecord::restore(network, digest, account, issued_at_ms, expires_at_ms, state)
        .map_err(Into::into)
}

fn encode_state(output: &mut Vec<u8>, state: ApprovalCodeState) {
    match state {
        ApprovalCodeState::Ready => output.push(0),
        ApprovalCodeState::Claimed(claim) => {
            output.push(1);
            encode_claim(output, claim);
        }
        ApprovalCodeState::Consumed {
            claim,
            consumed_at_ms,
        } => {
            output.push(2);
            encode_claim(output, claim);
            output.extend_from_slice(&consumed_at_ms.to_be_bytes());
        }
    }
}

fn encode_claim(output: &mut Vec<u8>, claim: ApprovalCodeClaim) {
    output.extend_from_slice(claim.checkout.as_bytes());
    output.extend_from_slice(&claim.claimed_at_ms.to_be_bytes());
}

fn decode_state(decoder: &mut Decoder<'_>) -> Result<ApprovalCodeState, ApprovalCodeStoreError> {
    match decoder.u8()? {
        0 => Ok(ApprovalCodeState::Ready),
        1 => Ok(ApprovalCodeState::Claimed(decode_claim(decoder)?)),
        2 => Ok(ApprovalCodeState::Consumed {
            claim: decode_claim(decoder)?,
            consumed_at_ms: decoder.u64()?,
        }),
        _ => Err(ApprovalCodeStoreError::CorruptRecord),
    }
}

fn decode_claim(decoder: &mut Decoder<'_>) -> Result<ApprovalCodeClaim, ApprovalCodeStoreError> {
    Ok(ApprovalCodeClaim {
        checkout: CheckoutId::new(decoder.array()?),
        claimed_at_ms: decoder.u64()?,
    })
}

struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], ApprovalCodeStoreError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
        let value = self
            .input
            .get(self.position..end)
            .ok_or(ApprovalCodeStoreError::CorruptRecord)?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ApprovalCodeStoreError> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| ApprovalCodeStoreError::CorruptRecord)
    }

    fn u8(&mut self) -> Result<u8, ApprovalCodeStoreError> {
        self.bytes(1)?
            .first()
            .copied()
            .ok_or(ApprovalCodeStoreError::CorruptRecord)
    }

    fn u64(&mut self) -> Result<u64, ApprovalCodeStoreError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn finish(self) -> Result<(), ApprovalCodeStoreError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(ApprovalCodeStoreError::CorruptRecord)
        }
    }
}
