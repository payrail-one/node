use external_asset_core::{
    DestinationCommitment, FinalityEvidence, OriginAssetId, OriginDeposit, OriginNetworkId,
    OriginReleaseReference, OriginTransactionId, SettlementReference, WithdrawalId,
    WithdrawalRequest,
};
use external_asset_settlement_core::SignedWithdrawalRequest;
use ledger_core::{AccountId, AssetId, Authorization, NetworkId, SignatureBytes};
use tail_sync_core::MAX_TAIL_PAYLOAD_BYTES;

use crate::{
    CompleteWithdrawalOperation, DepositSettlementOperation, ExternalSettlementOperation,
    ExternalSettlementRuntimeError, WithdrawalSettlementOperation,
};

const BLOCK_DOMAIN: &[u8; 32] = b"external.settlement.block.v1\0\0\0\0";

#[derive(Clone, Copy, Debug, Default)]
pub struct ExternalSettlementBlockCodec;

impl ExternalSettlementBlockCodec {
    #[must_use]
    pub fn is_external(input: &[u8]) -> bool {
        input.starts_with(BLOCK_DOMAIN)
    }

    /// Encodes exactly one authority operation in canonical consensus form.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload exceeds the consensus byte bound.
    pub fn encode(
        operation: ExternalSettlementOperation,
    ) -> Result<Vec<u8>, ExternalSettlementRuntimeError> {
        let mut output = Vec::with_capacity(640);
        output.extend_from_slice(BLOCK_DOMAIN);
        match operation {
            ExternalSettlementOperation::Deposit(operation) => {
                output.push(0);
                encode_network(&mut output, operation.network);
                crate::operation::encode_origin_deposit(&mut output, operation.claim);
                crate::operation::encode_finality(&mut output, operation.evidence);
                output.extend_from_slice(operation.reference.as_bytes());
                encode_authorization(&mut output, operation.observer_authorization);
                encode_authorization(&mut output, operation.finality_authorization);
                encode_authorization(&mut output, operation.settlement_authorization);
            }
            ExternalSettlementOperation::Withdrawal(operation) => {
                output.push(1);
                encode_signed_withdrawal(&mut output, operation.request);
                encode_authorization(&mut output, operation.settlement_authorization);
                encode_authorization(&mut output, operation.release_authorization);
            }
            ExternalSettlementOperation::CompleteWithdrawal(operation) => {
                output.push(2);
                encode_network(&mut output, operation.network);
                output.extend_from_slice(operation.withdrawal.as_bytes());
                output.extend_from_slice(operation.release_reference.transaction.as_bytes());
                output.extend_from_slice(&operation.release_reference.event_index.to_be_bytes());
                crate::operation::encode_finality(&mut output, operation.evidence);
                encode_authorization(&mut output, operation.release_authorization);
                encode_authorization(&mut output, operation.finality_authorization);
            }
        }
        if output.len() > MAX_TAIL_PAYLOAD_BYTES {
            return Err(ExternalSettlementRuntimeError::BlockTooLarge);
        }
        Ok(output)
    }

    /// Decodes one complete canonical authority operation.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, oversized or trailing input.
    pub fn decode(
        input: &[u8],
    ) -> Result<ExternalSettlementOperation, ExternalSettlementRuntimeError> {
        if input.len() > MAX_TAIL_PAYLOAD_BYTES {
            return Err(ExternalSettlementRuntimeError::BlockTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_array::<32>()? != *BLOCK_DOMAIN {
            return Err(ExternalSettlementRuntimeError::InvalidDomain);
        }
        let operation = match decoder.read_u8()? {
            0 => ExternalSettlementOperation::Deposit(DepositSettlementOperation {
                network: NetworkId::new(decoder.read_array()?),
                claim: decode_origin_deposit(&mut decoder)?,
                evidence: decode_finality(&mut decoder)?,
                reference: SettlementReference::new(decoder.read_array()?),
                observer_authorization: decode_authorization(&mut decoder)?,
                finality_authorization: decode_authorization(&mut decoder)?,
                settlement_authorization: decode_authorization(&mut decoder)?,
            }),
            1 => ExternalSettlementOperation::Withdrawal(WithdrawalSettlementOperation {
                request: decode_signed_withdrawal(&mut decoder)?,
                settlement_authorization: decode_authorization(&mut decoder)?,
                release_authorization: decode_authorization(&mut decoder)?,
            }),
            2 => ExternalSettlementOperation::CompleteWithdrawal(CompleteWithdrawalOperation {
                network: NetworkId::new(decoder.read_array()?),
                withdrawal: WithdrawalId::new(decoder.read_array()?),
                release_reference: OriginReleaseReference {
                    transaction: OriginTransactionId::new(decoder.read_array()?),
                    event_index: decoder.read_u32()?,
                },
                evidence: decode_finality(&mut decoder)?,
                release_authorization: decode_authorization(&mut decoder)?,
                finality_authorization: decode_authorization(&mut decoder)?,
            }),
            _ => return Err(ExternalSettlementRuntimeError::InvalidOperation),
        };
        decoder.finish()?;
        Ok(operation)
    }
}

fn encode_network(output: &mut Vec<u8>, network: NetworkId) {
    output.extend_from_slice(network.as_bytes());
}

fn encode_authorization(output: &mut Vec<u8>, authorization: Authorization) {
    output.extend_from_slice(authorization.signer.as_bytes());
    output.extend_from_slice(authorization.signature.as_bytes());
}

fn encode_signed_withdrawal(output: &mut Vec<u8>, signed: SignedWithdrawalRequest) {
    encode_network(output, signed.network);
    output.extend_from_slice(signed.request.asset.as_bytes());
    output.extend_from_slice(signed.request.account.as_bytes());
    output.extend_from_slice(&signed.request.amount.to_be_bytes());
    output.extend_from_slice(signed.request.destination.as_bytes());
    output.extend_from_slice(signed.request.burn_reference.as_bytes());
    output.extend_from_slice(&signed.valid_until_height.to_be_bytes());
    output.extend_from_slice(signed.signature.as_bytes());
}

fn decode_origin_deposit(
    decoder: &mut Decoder<'_>,
) -> Result<OriginDeposit, ExternalSettlementRuntimeError> {
    Ok(OriginDeposit {
        asset: AssetId::new(decoder.read_array()?),
        origin_network: OriginNetworkId::new(decoder.read_array()?),
        origin_asset: OriginAssetId::new(decoder.read_array()?),
        transaction: OriginTransactionId::new(decoder.read_array()?),
        event_index: decoder.read_u32()?,
        recipient: AccountId::new(decoder.read_array()?),
        amount: decoder.read_u128()?,
    })
}

fn decode_finality(
    decoder: &mut Decoder<'_>,
) -> Result<FinalityEvidence, ExternalSettlementRuntimeError> {
    Ok(FinalityEvidence {
        block_height: decoder.read_u64()?,
        block_hash: decoder.read_array()?,
        proof_hash: decoder.read_array()?,
    })
}

fn decode_authorization(
    decoder: &mut Decoder<'_>,
) -> Result<Authorization, ExternalSettlementRuntimeError> {
    Ok(Authorization {
        signer: AccountId::new(decoder.read_array()?),
        signature: SignatureBytes::new(decoder.read_array()?),
    })
}

fn decode_signed_withdrawal(
    decoder: &mut Decoder<'_>,
) -> Result<SignedWithdrawalRequest, ExternalSettlementRuntimeError> {
    Ok(SignedWithdrawalRequest {
        network: NetworkId::new(decoder.read_array()?),
        request: WithdrawalRequest {
            asset: AssetId::new(decoder.read_array()?),
            account: AccountId::new(decoder.read_array()?),
            amount: decoder.read_u128()?,
            destination: DestinationCommitment::new(decoder.read_array()?),
            burn_reference: SettlementReference::new(decoder.read_array()?),
        },
        valid_until_height: decoder.read_u64()?,
        signature: SignatureBytes::new(decoder.read_array()?),
    })
}

struct Decoder<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, ExternalSettlementRuntimeError> {
        self.read_slice(1).map(|bytes| bytes[0])
    }

    fn read_u32(&mut self) -> Result<u32, ExternalSettlementRuntimeError> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, ExternalSettlementRuntimeError> {
        Ok(u64::from_be_bytes(self.read_array()?))
    }

    fn read_u128(&mut self) -> Result<u128, ExternalSettlementRuntimeError> {
        Ok(u128::from_be_bytes(self.read_array()?))
    }

    fn read_array<const LENGTH: usize>(
        &mut self,
    ) -> Result<[u8; LENGTH], ExternalSettlementRuntimeError> {
        self.read_slice(LENGTH)?
            .try_into()
            .map_err(|_| ExternalSettlementRuntimeError::UnexpectedEnd)
    }

    fn read_slice(&mut self, length: usize) -> Result<&'a [u8], ExternalSettlementRuntimeError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ExternalSettlementRuntimeError::UnexpectedEnd)?;
        let value = self
            .input
            .get(self.offset..end)
            .ok_or(ExternalSettlementRuntimeError::UnexpectedEnd)?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), ExternalSettlementRuntimeError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(ExternalSettlementRuntimeError::TrailingBytes)
        }
    }
}
