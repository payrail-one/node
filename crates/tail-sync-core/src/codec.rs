use ledger_core::NetworkId;
use state_sync_core::{
    BlockHash, FinalizedCheckpoint, MAX_FINALITY_PROOF_BYTES, StateRoot, ValidatorSetHash,
};

use crate::{FinalizedTailBlock, MAX_TAIL_PAYLOAD_BYTES, TailSyncError};

const DOMAIN: &[u8; 16] = b"tail.block.v1.0\0";
const FIXED_BYTES: usize = 16 + 32 + 32 + 8 + 32 + 32 + 32 + 4 + 4;
pub const MAX_ENCODED_TAIL_BLOCK_BYTES: usize =
    FIXED_BYTES + MAX_TAIL_PAYLOAD_BYTES + MAX_FINALITY_PROOF_BYTES;

#[derive(Clone, Copy, Debug, Default)]
pub struct TailBlockCodec;

impl TailBlockCodec {
    /// Encodes one canonical bounded tail block.
    ///
    /// # Errors
    ///
    /// Returns an error if payload or proof exceeds its bound.
    pub fn encode(block: &FinalizedTailBlock) -> Result<Vec<u8>, TailSyncError> {
        if block.payload.len() > MAX_TAIL_PAYLOAD_BYTES {
            return Err(TailSyncError::PayloadTooLarge);
        }
        if block.finality_proof.len() > MAX_FINALITY_PROOF_BYTES {
            return Err(TailSyncError::FinalityProofTooLarge);
        }
        let payload_length =
            u32::try_from(block.payload.len()).map_err(|_| TailSyncError::PayloadTooLarge)?;
        let proof_length = u32::try_from(block.finality_proof.len())
            .map_err(|_| TailSyncError::FinalityProofTooLarge)?;
        let mut output =
            Vec::with_capacity(FIXED_BYTES + block.payload.len() + block.finality_proof.len());
        output.extend_from_slice(DOMAIN);
        output.extend_from_slice(block.network.as_bytes());
        output.extend_from_slice(block.parent_hash.as_bytes());
        output.extend_from_slice(&block.checkpoint.height.to_be_bytes());
        output.extend_from_slice(block.checkpoint.block_hash.as_bytes());
        output.extend_from_slice(block.checkpoint.state_root.as_bytes());
        output.extend_from_slice(block.checkpoint.validator_set_hash.as_bytes());
        output.extend_from_slice(&payload_length.to_be_bytes());
        output.extend_from_slice(&proof_length.to_be_bytes());
        output.extend_from_slice(&block.payload);
        output.extend_from_slice(&block.finality_proof);
        Ok(output)
    }

    /// Decodes one complete canonical bounded tail block.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, truncated, trailing or wrong-domain data.
    pub fn decode(input: &[u8]) -> Result<FinalizedTailBlock, TailSyncError> {
        if input.len() > MAX_ENCODED_TAIL_BLOCK_BYTES {
            return Err(TailSyncError::PayloadTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.bytes(DOMAIN.len())? != DOMAIN {
            return Err(TailSyncError::InvalidDomain);
        }
        let network = NetworkId::new(decoder.array()?);
        let parent_hash = BlockHash::new(decoder.array()?);
        let checkpoint = FinalizedCheckpoint {
            height: u64::from_be_bytes(decoder.array()?),
            block_hash: BlockHash::new(decoder.array()?),
            state_root: StateRoot::new(decoder.array()?),
            validator_set_hash: ValidatorSetHash::new(decoder.array()?),
        };
        let payload_length = usize::try_from(u32::from_be_bytes(decoder.array()?))
            .map_err(|_| TailSyncError::PayloadTooLarge)?;
        let proof_length = usize::try_from(u32::from_be_bytes(decoder.array()?))
            .map_err(|_| TailSyncError::FinalityProofTooLarge)?;
        if payload_length > MAX_TAIL_PAYLOAD_BYTES {
            return Err(TailSyncError::PayloadTooLarge);
        }
        if proof_length > MAX_FINALITY_PROOF_BYTES {
            return Err(TailSyncError::FinalityProofTooLarge);
        }
        let payload = decoder.bytes(payload_length)?.to_vec();
        let finality_proof = decoder.bytes(proof_length)?.to_vec();
        decoder.finish()?;
        Ok(FinalizedTailBlock {
            network,
            parent_hash,
            checkpoint,
            payload,
            finality_proof,
        })
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

    fn array<const N: usize>(&mut self) -> Result<[u8; N], TailSyncError> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| TailSyncError::UnexpectedEnd)
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], TailSyncError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(TailSyncError::UnexpectedEnd)?;
        let bytes = self
            .input
            .get(self.position..end)
            .ok_or(TailSyncError::UnexpectedEnd)?;
        self.position = end;
        Ok(bytes)
    }

    fn finish(self) -> Result<(), TailSyncError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(TailSyncError::TrailingBytes)
        }
    }
}
