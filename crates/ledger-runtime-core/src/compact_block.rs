use ledger_core::OperationId;
use sha2::{Digest, Sha256};
use tail_sync_core::MAX_TAIL_PAYLOAD_BYTES;

use crate::{LedgerBlockCodec, MAX_BLOCK_OPERATIONS, RuntimeError, decoder::Decoder};

const MANIFEST_DOMAIN: &[u8] = b"ledger.compact-block.v1\0";
const PAYLOAD_HASH_DOMAIN: &[u8] = b"ledger.block-payload.v1\0";
const OPERATION_ID_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockPayloadHash([u8; 32]);

impl BlockPayloadHash {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Compact, canonical commitment to one complete ledger block payload.
///
/// Consensus may carry this manifest after transaction gossip has distributed
/// the envelopes. A validator must reconstruct and verify the exact payload
/// before voting; the manifest alone is never an execution capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactLedgerBlockManifest {
    payload_length: u32,
    payload_hash: BlockPayloadHash,
    operations: Vec<OperationId>,
}

impl CompactLedgerBlockManifest {
    /// Builds a manifest from a complete canonical ledger block.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized or non-canonical payloads.
    pub fn from_payload(payload: &[u8]) -> Result<Self, RuntimeError> {
        let payload_length =
            u32::try_from(payload.len()).map_err(|_| RuntimeError::BlockTooLarge)?;
        let operations = operation_ids(payload)?;
        Ok(Self {
            payload_length,
            payload_hash: block_payload_hash(payload),
            operations,
        })
    }

    #[must_use]
    pub const fn payload_length(&self) -> u32 {
        self.payload_length
    }

    #[must_use]
    pub const fn payload_hash(&self) -> BlockPayloadHash {
        self.payload_hash
    }

    #[must_use]
    pub fn operations(&self) -> &[OperationId] {
        &self.operations
    }

    #[must_use]
    pub fn encoded_len(&self) -> usize {
        MANIFEST_DOMAIN.len() + 4 + 32 + 4 + (self.operations.len() * OPERATION_ID_BYTES)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.encoded_len());
        output.extend_from_slice(MANIFEST_DOMAIN);
        output.extend_from_slice(&self.payload_length.to_be_bytes());
        output.extend_from_slice(self.payload_hash.as_bytes());
        output.extend_from_slice(
            &u32::try_from(self.operations.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        for operation in &self.operations {
            output.extend_from_slice(operation.as_bytes());
        }
        output
    }

    /// Decodes one complete bounded manifest.
    ///
    /// # Errors
    ///
    /// Rejects malformed, truncated, trailing or out-of-range fields.
    pub fn decode(input: &[u8]) -> Result<Self, RuntimeError> {
        if input.len() > maximum_manifest_bytes() {
            return Err(RuntimeError::BlockTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_slice(MANIFEST_DOMAIN.len())? != MANIFEST_DOMAIN {
            return Err(RuntimeError::InvalidDomain);
        }
        let payload_length = decoder.read_u32()?;
        if usize::try_from(payload_length).map_err(|_| RuntimeError::BlockTooLarge)?
            > MAX_TAIL_PAYLOAD_BYTES
        {
            return Err(RuntimeError::BlockTooLarge);
        }
        let payload_hash = BlockPayloadHash::new(decoder.read_array()?);
        let count =
            usize::try_from(decoder.read_u32()?).map_err(|_| RuntimeError::TooManyOperations)?;
        if count > MAX_BLOCK_OPERATIONS {
            return Err(RuntimeError::TooManyOperations);
        }
        let mut operations = Vec::with_capacity(count);
        for _ in 0..count {
            operations.push(OperationId::new(decoder.read_array()?));
        }
        decoder.finish()?;
        Ok(Self {
            payload_length,
            payload_hash,
            operations,
        })
    }

    /// Checks that bytes reconstruct the exact committed canonical payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the supplied payload is not a canonical block.
    pub fn matches_payload(&self, payload: &[u8]) -> Result<bool, RuntimeError> {
        let length_matches = usize::try_from(self.payload_length)
            .map(|length| length == payload.len())
            .unwrap_or(false);
        if !length_matches || block_payload_hash(payload) != self.payload_hash {
            return Ok(false);
        }
        Ok(operation_ids(payload)? == self.operations)
    }
}

#[must_use]
pub fn block_payload_hash(payload: &[u8]) -> BlockPayloadHash {
    let mut hasher = Sha256::new();
    hasher.update(PAYLOAD_HASH_DOMAIN);
    hasher.update(payload);
    BlockPayloadHash::new(hasher.finalize().into())
}

fn operation_ids(payload: &[u8]) -> Result<Vec<OperationId>, RuntimeError> {
    LedgerBlockCodec::decode(payload)?
        .into_iter()
        .map(|signed| {
            signed
                .operation
                .operation_id()
                .map_err(RuntimeError::InvalidOperationId)
        })
        .collect()
}

const fn maximum_manifest_bytes() -> usize {
    MANIFEST_DOMAIN.len() + 4 + 32 + 4 + (MAX_BLOCK_OPERATIONS * OPERATION_ID_BYTES)
}
