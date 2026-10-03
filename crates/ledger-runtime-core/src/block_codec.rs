use ledger_core::SignedOperation;
use tail_sync_core::MAX_TAIL_PAYLOAD_BYTES;
use transaction_protocol::{MAX_ENVELOPE_BYTES, SignedOperationCodec};

use crate::{RuntimeError, decoder::Decoder};

const BLOCK_DOMAIN: &[u8; 16] = b"ledger.block.v1\0";
/// Independent operation-count bound below the four-megabyte payload bound.
/// A canonical simple transfer is roughly 326 bytes, so 12,288 transfers fit
/// while larger envelopes are still rejected by the byte limit during encode.
pub const MAX_BLOCK_OPERATIONS: usize = 12_288;

#[derive(Clone, Copy, Debug, Default)]
pub struct LedgerBlockCodec;

impl LedgerBlockCodec {
    /// Encodes signed operations in their consensus order.
    ///
    /// # Errors
    ///
    /// Returns an error when an operation or the final block exceeds a bound.
    pub fn encode(operations: &[SignedOperation]) -> Result<Vec<u8>, RuntimeError> {
        if operations.len() > MAX_BLOCK_OPERATIONS {
            return Err(RuntimeError::TooManyOperations);
        }
        let count = u32::try_from(operations.len()).map_err(|_| RuntimeError::TooManyOperations)?;
        let mut output = Vec::new();
        output.extend_from_slice(BLOCK_DOMAIN);
        output.extend_from_slice(&count.to_be_bytes());
        for operation in operations {
            let envelope =
                SignedOperationCodec::encode(operation).map_err(RuntimeError::InvalidEnvelope)?;
            let length =
                u32::try_from(envelope.len()).map_err(|_| RuntimeError::EnvelopeTooLarge)?;
            output.extend_from_slice(&length.to_be_bytes());
            output.extend_from_slice(&envelope);
            if output.len() > MAX_TAIL_PAYLOAD_BYTES {
                return Err(RuntimeError::BlockTooLarge);
            }
        }
        Ok(output)
    }

    /// Reconstructs a canonical block from already-gossiped exact envelopes.
    ///
    /// Every envelope is decoded through the authoritative transaction codec
    /// before publication. Callers must still compare the completed payload to
    /// the compact manifest and execute it before voting.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed envelope or operation/payload bound.
    pub fn encode_envelopes(envelopes: &[&[u8]]) -> Result<Vec<u8>, RuntimeError> {
        if envelopes.len() > MAX_BLOCK_OPERATIONS {
            return Err(RuntimeError::TooManyOperations);
        }
        let count = u32::try_from(envelopes.len()).map_err(|_| RuntimeError::TooManyOperations)?;
        let mut output = Vec::new();
        output.extend_from_slice(BLOCK_DOMAIN);
        output.extend_from_slice(&count.to_be_bytes());
        for envelope in envelopes {
            if envelope.len() > MAX_ENVELOPE_BYTES {
                return Err(RuntimeError::EnvelopeTooLarge);
            }
            SignedOperationCodec::decode(envelope).map_err(RuntimeError::InvalidEnvelope)?;
            let length =
                u32::try_from(envelope.len()).map_err(|_| RuntimeError::EnvelopeTooLarge)?;
            output.extend_from_slice(&length.to_be_bytes());
            output.extend_from_slice(envelope);
            if output.len() > MAX_TAIL_PAYLOAD_BYTES {
                return Err(RuntimeError::BlockTooLarge);
            }
        }
        Ok(output)
    }

    /// Decodes a complete bounded block and rejects trailing or ambiguous data.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized or non-canonical input.
    pub fn decode(input: &[u8]) -> Result<Vec<SignedOperation>, RuntimeError> {
        if input.len() > MAX_TAIL_PAYLOAD_BYTES {
            return Err(RuntimeError::BlockTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_array::<16>()? != *BLOCK_DOMAIN {
            return Err(RuntimeError::InvalidDomain);
        }
        let count =
            usize::try_from(decoder.read_u32()?).map_err(|_| RuntimeError::TooManyOperations)?;
        if count > MAX_BLOCK_OPERATIONS {
            return Err(RuntimeError::TooManyOperations);
        }
        let mut operations = Vec::with_capacity(count);
        for _ in 0..count {
            let length =
                usize::try_from(decoder.read_u32()?).map_err(|_| RuntimeError::EnvelopeTooLarge)?;
            if length > MAX_ENVELOPE_BYTES {
                return Err(RuntimeError::EnvelopeTooLarge);
            }
            let envelope = decoder.read_slice(length)?;
            operations.push(
                SignedOperationCodec::decode(envelope).map_err(RuntimeError::InvalidEnvelope)?,
            );
        }
        decoder.finish()?;
        Ok(operations)
    }
}
