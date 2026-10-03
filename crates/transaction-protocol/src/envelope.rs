use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, ContractCall, ContractDeploy,
    ContractId, IdempotencyKey, LedgerError, MAX_BATCH_ITEMS, MAX_CONTRACT_ARGS_BYTES,
    MAX_CONTRACT_CODE_BYTES, NetworkId, SignatureBytes, SignedOperation, Transfer, TransferBatch,
    TransferItem,
};

use crate::{ProtocolError, decoder::Decoder};

const ENVELOPE_DOMAIN: &[u8; 16] = b"ledger.envelope\0";
pub const MAX_ENVELOPE_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct SignedOperationCodec;

impl SignedOperationCodec {
    /// Encodes a signed operation using the bounded canonical envelope format.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation or final envelope exceeds protocol
    /// bounds.
    pub fn encode(signed: &SignedOperation) -> Result<Vec<u8>, ProtocolError> {
        if signed.operation.fee_payer().is_some() != signed.fee_payer_authorization.is_some() {
            return Err(ProtocolError::AuthorizationSetMismatch);
        }
        let operation = signed
            .operation
            .canonical_bytes()
            .map_err(|error| map_ledger_error(&error))?;
        let mut output = Vec::new();
        output.extend_from_slice(ENVELOPE_DOMAIN);
        output.extend_from_slice(&operation);
        encode_authorization(&mut output, signed.sender_authorization);
        match signed.fee_payer_authorization {
            None => output.push(0),
            Some(authorization) => {
                output.push(1);
                encode_authorization(&mut output, authorization);
            }
        }
        if output.len() > MAX_ENVELOPE_BYTES {
            return Err(ProtocolError::EnvelopeTooLarge);
        }
        Ok(output)
    }

    /// Decodes one complete signed operation and rejects ambiguous trailing
    /// data, unsupported variants and allocations above the batch bound.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, truncated, oversized or non-canonical
    /// envelope bytes.
    pub fn decode(input: &[u8]) -> Result<SignedOperation, ProtocolError> {
        if input.len() > MAX_ENVELOPE_BYTES {
            return Err(ProtocolError::EnvelopeTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_array::<16>()? != *ENVELOPE_DOMAIN {
            return Err(ProtocolError::InvalidDomain);
        }
        let operation = decode_operation(&mut decoder)?;
        let sender_authorization = decode_authorization(&mut decoder)?;
        let fee_payer_authorization = match decoder.read_u8()? {
            0 => None,
            1 => Some(decode_authorization(&mut decoder)?),
            _ => return Err(ProtocolError::InvalidAuthorizationFlag),
        };
        if operation.fee_payer().is_some() != fee_payer_authorization.is_some() {
            return Err(ProtocolError::AuthorizationSetMismatch);
        }
        decoder.finish()?;
        Ok(SignedOperation {
            operation,
            sender_authorization,
            fee_payer_authorization,
        })
    }
}

fn map_ledger_error(error: &LedgerError) -> ProtocolError {
    match error {
        LedgerError::BatchTooLarge => ProtocolError::BatchTooLarge,
        _ => ProtocolError::InvalidOperation,
    }
}

fn encode_authorization(output: &mut Vec<u8>, authorization: Authorization) {
    output.extend_from_slice(authorization.signer.as_bytes());
    output.extend_from_slice(authorization.signature.as_bytes());
}

fn decode_authorization(decoder: &mut Decoder<'_>) -> Result<Authorization, ProtocolError> {
    Ok(Authorization {
        signer: AccountId::new(decoder.read_array()?),
        signature: SignatureBytes::new(decoder.read_array()?),
    })
}

fn decode_operation(decoder: &mut Decoder<'_>) -> Result<AuthorizedOperation, ProtocolError> {
    match decoder.read_u8()? {
        0 => Ok(AuthorizedOperation::Transfer(decode_transfer(decoder)?)),
        1 => Ok(AuthorizedOperation::TransferBatch(decode_batch(decoder)?)),
        2 => Ok(AuthorizedOperation::SponsoredTransfer {
            transfer: decode_transfer(decoder)?,
            fee_payer: AccountId::new(decoder.read_array()?),
        }),
        3 => Ok(AuthorizedOperation::SponsoredBatchTransfer {
            batch: decode_batch(decoder)?,
            fee_payer: AccountId::new(decoder.read_array()?),
        }),
        4 => Ok(AuthorizedOperation::ContractDeploy(decode_contract_deploy(
            decoder,
        )?)),
        5 => Ok(AuthorizedOperation::ContractCall(decode_contract_call(
            decoder,
        )?)),
        _ => Err(ProtocolError::UnsupportedOperation),
    }
}

fn decode_contract_deploy(decoder: &mut Decoder<'_>) -> Result<ContractDeploy, ProtocolError> {
    let network = NetworkId::new(decoder.read_array()?);
    let idempotency_key = IdempotencyKey::new(decoder.read_array()?);
    let asset = AssetId::new(decoder.read_array()?);
    let owner = AccountId::new(decoder.read_array()?);
    let salt = decoder.read_array()?;
    let code_length =
        usize::try_from(decoder.read_u32()?).map_err(|_| ProtocolError::EnvelopeTooLarge)?;
    if code_length == 0 || code_length > MAX_CONTRACT_CODE_BYTES {
        return Err(ProtocolError::InvalidOperation);
    }
    let code = decoder.read_slice(code_length)?.to_vec();
    Ok(ContractDeploy {
        network,
        idempotency_key,
        asset,
        owner,
        salt,
        code,
        fee: decoder.read_u128()?,
        nonce: decoder.read_u64()?,
        valid_until_height: decoder.read_u64()?,
    })
}

fn decode_contract_call(decoder: &mut Decoder<'_>) -> Result<ContractCall, ProtocolError> {
    let network = NetworkId::new(decoder.read_array()?);
    let idempotency_key = IdempotencyKey::new(decoder.read_array()?);
    let asset = AssetId::new(decoder.read_array()?);
    let caller = AccountId::new(decoder.read_array()?);
    let contract = ContractId::new(decoder.read_array()?);
    let entrypoint_length = usize::from(decoder.read_u8()?);
    if entrypoint_length == 0 || entrypoint_length > 32 {
        return Err(ProtocolError::InvalidOperation);
    }
    let entrypoint = std::str::from_utf8(decoder.read_slice(entrypoint_length)?)
        .map_err(|_| ProtocolError::InvalidOperation)?
        .to_owned();
    let args_length =
        usize::try_from(decoder.read_u32()?).map_err(|_| ProtocolError::EnvelopeTooLarge)?;
    if args_length > MAX_CONTRACT_ARGS_BYTES {
        return Err(ProtocolError::InvalidOperation);
    }
    let args = decoder.read_slice(args_length)?.to_vec();
    Ok(ContractCall {
        network,
        idempotency_key,
        asset,
        caller,
        contract,
        entrypoint,
        args,
        attached_amount: decoder.read_u128()?,
        fee: decoder.read_u128()?,
        execution_limit: decoder.read_u64()?,
        nonce: decoder.read_u64()?,
        valid_until_height: decoder.read_u64()?,
    })
}

fn decode_transfer(decoder: &mut Decoder<'_>) -> Result<Transfer, ProtocolError> {
    Ok(Transfer {
        network: NetworkId::new(decoder.read_array()?),
        idempotency_key: IdempotencyKey::new(decoder.read_array()?),
        asset: AssetId::new(decoder.read_array()?),
        from: AccountId::new(decoder.read_array()?),
        to: AccountId::new(decoder.read_array()?),
        amount: decoder.read_u128()?,
        fee: decoder.read_u128()?,
        nonce: decoder.read_u64()?,
        valid_until_height: decoder.read_u64()?,
    })
}

fn decode_batch(decoder: &mut Decoder<'_>) -> Result<TransferBatch, ProtocolError> {
    let network = NetworkId::new(decoder.read_array()?);
    let idempotency_key = IdempotencyKey::new(decoder.read_array()?);
    let asset = AssetId::new(decoder.read_array()?);
    let from = AccountId::new(decoder.read_array()?);
    let item_count =
        usize::try_from(decoder.read_u32()?).map_err(|_| ProtocolError::BatchTooLarge)?;
    if item_count > MAX_BATCH_ITEMS {
        return Err(ProtocolError::BatchTooLarge);
    }
    let mut items = Vec::with_capacity(item_count);
    for _ in 0..item_count {
        items.push(TransferItem {
            to: AccountId::new(decoder.read_array()?),
            amount: decoder.read_u128()?,
        });
    }
    Ok(TransferBatch {
        network,
        idempotency_key,
        asset,
        from,
        items,
        fee: decoder.read_u128()?,
        nonce: decoder.read_u64()?,
        valid_until_height: decoder.read_u64()?,
    })
}
