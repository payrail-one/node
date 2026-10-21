use external_asset_core::{
    ExternalAssetStateCodec, ExternalAssetStateCodecError, MAX_EXTERNAL_ASSET_STATE_BYTES,
};
use external_asset_settlement_core::{
    ExternalAssetSettlement, ExternalAssetSettlementSnapshot, ExternalSettlementError,
};
use ledger_core::NetworkId;
use ledger_runtime_core::{LedgerStateCodec, MAX_LEDGER_STATE_BYTES, RuntimeError};

const STATE_DOMAIN: &[u8; 32] = b"external.settlement.state.v1\0\0\0\0";
pub const MAX_EXTERNAL_SETTLEMENT_STATE_BYTES: usize =
    MAX_LEDGER_STATE_BYTES + MAX_EXTERNAL_ASSET_STATE_BYTES + 40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalSettlementStateCodecError {
    StateTooLarge,
    InvalidDomain,
    InvalidLength,
    UnexpectedEnd,
    TrailingBytes,
    Ledger(RuntimeError),
    External(ExternalAssetStateCodecError),
    Settlement(ExternalSettlementError),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExternalSettlementStateCodec;

impl ExternalSettlementStateCodec {
    /// Encodes and cross-validates the combined consensus state.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid component state, reconciliation failure or
    /// a size that cannot be represented by the canonical framing.
    pub fn encode(
        snapshot: &ExternalAssetSettlementSnapshot,
    ) -> Result<Vec<u8>, ExternalSettlementStateCodecError> {
        let network = snapshot.ledger.network;
        let canonical =
            ExternalAssetSettlement::from_snapshot(network, snapshot.clone())?.snapshot();
        let ledger = LedgerStateCodec::encode(&canonical.ledger)?;
        let external = ExternalAssetStateCodec::encode(&canonical.external)?;
        let ledger_length = u32::try_from(ledger.len())
            .map_err(|_| ExternalSettlementStateCodecError::InvalidLength)?;
        let external_length = u32::try_from(external.len())
            .map_err(|_| ExternalSettlementStateCodecError::InvalidLength)?;
        let capacity = STATE_DOMAIN
            .len()
            .checked_add(8)
            .and_then(|value| value.checked_add(ledger.len()))
            .and_then(|value| value.checked_add(external.len()))
            .ok_or(ExternalSettlementStateCodecError::StateTooLarge)?;
        if capacity > MAX_EXTERNAL_SETTLEMENT_STATE_BYTES {
            return Err(ExternalSettlementStateCodecError::StateTooLarge);
        }
        let mut output = Vec::with_capacity(capacity);
        output.extend_from_slice(STATE_DOMAIN);
        output.extend_from_slice(&ledger_length.to_be_bytes());
        output.extend_from_slice(&ledger);
        output.extend_from_slice(&external_length.to_be_bytes());
        output.extend_from_slice(&external);
        Ok(output)
    }

    /// Decodes, validates and cross-reconciles the combined consensus state.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, malformed, trailing or inconsistent
    /// component state.
    pub fn decode(
        network: NetworkId,
        input: &[u8],
    ) -> Result<ExternalAssetSettlementSnapshot, ExternalSettlementStateCodecError> {
        if input.len() > MAX_EXTERNAL_SETTLEMENT_STATE_BYTES {
            return Err(ExternalSettlementStateCodecError::StateTooLarge);
        }
        let mut offset = 0_usize;
        if read(input, &mut offset, STATE_DOMAIN.len())? != STATE_DOMAIN {
            return Err(ExternalSettlementStateCodecError::InvalidDomain);
        }
        let ledger_length = read_length(input, &mut offset, MAX_LEDGER_STATE_BYTES)?;
        let ledger = LedgerStateCodec::decode(network, read(input, &mut offset, ledger_length)?)?;
        let external_length = read_length(input, &mut offset, MAX_EXTERNAL_ASSET_STATE_BYTES)?;
        let external = ExternalAssetStateCodec::decode(read(input, &mut offset, external_length)?)?;
        if offset != input.len() {
            return Err(ExternalSettlementStateCodecError::TrailingBytes);
        }
        ExternalAssetSettlement::from_snapshot(
            network,
            ExternalAssetSettlementSnapshot { ledger, external },
        )
        .map(|state| state.snapshot())
        .map_err(Into::into)
    }
}

fn read_length(
    input: &[u8],
    offset: &mut usize,
    maximum: usize,
) -> Result<usize, ExternalSettlementStateCodecError> {
    let encoded: [u8; 4] = read(input, offset, 4)?
        .try_into()
        .map_err(|_| ExternalSettlementStateCodecError::UnexpectedEnd)?;
    let length = usize::try_from(u32::from_be_bytes(encoded))
        .map_err(|_| ExternalSettlementStateCodecError::InvalidLength)?;
    if length > maximum {
        return Err(ExternalSettlementStateCodecError::InvalidLength);
    }
    Ok(length)
}

fn read<'a>(
    input: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ExternalSettlementStateCodecError> {
    let end = offset
        .checked_add(length)
        .ok_or(ExternalSettlementStateCodecError::UnexpectedEnd)?;
    let value = input
        .get(*offset..end)
        .ok_or(ExternalSettlementStateCodecError::UnexpectedEnd)?;
    *offset = end;
    Ok(value)
}

impl From<RuntimeError> for ExternalSettlementStateCodecError {
    fn from(error: RuntimeError) -> Self {
        Self::Ledger(error)
    }
}

impl From<ExternalAssetStateCodecError> for ExternalSettlementStateCodecError {
    fn from(error: ExternalAssetStateCodecError) -> Self {
        Self::External(error)
    }
}

impl From<ExternalSettlementError> for ExternalSettlementStateCodecError {
    fn from(error: ExternalSettlementError) -> Self {
        Self::Settlement(error)
    }
}
