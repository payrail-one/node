use ledger_core::NetworkId;
use network_membership_core::NodeId;
use state_sync_core::{
    BlockHash, FinalizedCheckpoint, MAX_FINALITY_PROOF_BYTES, StateRoot, ValidatorSetHash,
};

use crate::{
    ConsensusSignature, FinalityCertificate, FinalityError, MAX_VALIDATORS, ValidatorSignature,
};

const PROOF_DOMAIN: &[u8] = b"finality.proof\0";

#[derive(Clone, Copy, Debug, Default)]
pub struct FinalityCertificateCodec;

impl FinalityCertificateCodec {
    /// Encodes a bounded canonical finality certificate.
    ///
    /// # Errors
    ///
    /// Returns an error for empty, oversized or non-canonical signatures.
    pub fn encode(certificate: &FinalityCertificate) -> Result<Vec<u8>, FinalityError> {
        validate_signatures(&certificate.signatures)?;
        let mut output = Vec::new();
        output.extend_from_slice(PROOF_DOMAIN);
        output.extend_from_slice(certificate.network.as_bytes());
        output.extend_from_slice(&certificate.membership_epoch.to_be_bytes());
        output.extend_from_slice(&certificate.round.to_be_bytes());
        output.extend_from_slice(&certificate.checkpoint.height.to_be_bytes());
        output.extend_from_slice(certificate.checkpoint.block_hash.as_bytes());
        output.extend_from_slice(certificate.checkpoint.state_root.as_bytes());
        output.extend_from_slice(certificate.checkpoint.validator_set_hash.as_bytes());
        let count = u32::try_from(certificate.signatures.len())
            .map_err(|_| FinalityError::TooManySignatures)?;
        output.extend_from_slice(&count.to_be_bytes());
        for signature in &certificate.signatures {
            output.extend_from_slice(signature.node.as_bytes());
            output.extend_from_slice(signature.signature.as_bytes());
        }
        if output.len() > MAX_FINALITY_PROOF_BYTES {
            return Err(FinalityError::ProofTooLarge);
        }
        Ok(output)
    }

    /// Decodes one complete bounded certificate.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized, truncated, trailing or
    /// non-canonical proof bytes.
    pub fn decode(input: &[u8]) -> Result<FinalityCertificate, FinalityError> {
        if input.len() > MAX_FINALITY_PROOF_BYTES {
            return Err(FinalityError::ProofTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.bytes(PROOF_DOMAIN.len())? != PROOF_DOMAIN {
            return Err(FinalityError::InvalidDomain);
        }
        let network = NetworkId::new(decoder.array()?);
        let membership_epoch = u64::from_be_bytes(decoder.array()?);
        let round = u64::from_be_bytes(decoder.array()?);
        let checkpoint = FinalizedCheckpoint {
            height: u64::from_be_bytes(decoder.array()?),
            block_hash: BlockHash::new(decoder.array()?),
            state_root: StateRoot::new(decoder.array()?),
            validator_set_hash: ValidatorSetHash::new(decoder.array()?),
        };
        let count = usize::try_from(u32::from_be_bytes(decoder.array()?))
            .map_err(|_| FinalityError::TooManySignatures)?;
        if count == 0 {
            return Err(FinalityError::EmptyCertificate);
        }
        if count > MAX_VALIDATORS {
            return Err(FinalityError::TooManySignatures);
        }
        let mut signatures = Vec::with_capacity(count);
        for _ in 0..count {
            signatures.push(ValidatorSignature {
                node: NodeId::new(decoder.array()?),
                signature: ConsensusSignature::new(decoder.array()?),
            });
        }
        decoder.finish()?;
        validate_signatures(&signatures)?;
        Ok(FinalityCertificate {
            network,
            membership_epoch,
            round,
            checkpoint,
            signatures,
        })
    }
}

fn validate_signatures(signatures: &[ValidatorSignature]) -> Result<(), FinalityError> {
    if signatures.is_empty() {
        return Err(FinalityError::EmptyCertificate);
    }
    if signatures.len() > MAX_VALIDATORS {
        return Err(FinalityError::TooManySignatures);
    }
    if signatures
        .windows(2)
        .any(|pair| pair[0].node >= pair[1].node)
    {
        return Err(FinalityError::NonCanonicalSignatureOrder);
    }
    Ok(())
}

struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], FinalityError> {
        self.bytes(N)?
            .try_into()
            .map_err(|_| FinalityError::UnexpectedEnd)
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], FinalityError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(FinalityError::UnexpectedEnd)?;
        let bytes = self
            .input
            .get(self.position..end)
            .ok_or(FinalityError::UnexpectedEnd)?;
        self.position = end;
        Ok(bytes)
    }

    fn finish(self) -> Result<(), FinalityError> {
        if self.position == self.input.len() {
            Ok(())
        } else {
            Err(FinalityError::TrailingBytes)
        }
    }
}
