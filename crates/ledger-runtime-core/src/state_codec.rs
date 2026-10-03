use ledger_core::{LedgerSnapshot, NetworkId};

use crate::{LedgerRow, LedgerStateRows, RuntimeError, decoder::Decoder};

const STATE_DOMAIN: &[u8; 16] = b"ledger.state.v2\0";
const MAX_STATE_ENTRIES: usize = 1_000_000;
const MAX_ROW_KEY_BYTES: usize = 64;
const MAX_ROW_VALUE_BYTES: usize = 512;
pub const MAX_LEDGER_STATE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct LedgerStateCodec;

impl LedgerStateCodec {
    /// Encodes a validated ledger snapshot in deterministic key order.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot violates ledger invariants or codec bounds.
    pub fn encode(snapshot: &LedgerSnapshot) -> Result<Vec<u8>, RuntimeError> {
        let rows = LedgerStateRows::from_snapshot(snapshot)?;
        let mut output = Vec::new();
        output.extend_from_slice(STATE_DOMAIN);
        output.extend_from_slice(rows.network.as_bytes());
        output.extend_from_slice(rows.registry_authority.as_bytes());
        encode_rows(&mut output, &rows.assets)?;
        encode_rows(&mut output, &rows.balances)?;
        encode_rows(&mut output, &rows.nonces)?;
        encode_rows(&mut output, &rows.operation_sequence)?;
        encode_rows(&mut output, &rows.account_statuses)?;
        if output.len() > MAX_LEDGER_STATE_BYTES {
            return Err(RuntimeError::StateTooLarge);
        }
        Ok(output)
    }

    /// Decodes and fully validates a canonical ledger snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt, oversized, wrong-network or invariant-violating state.
    pub fn decode(
        expected_network: NetworkId,
        input: &[u8],
    ) -> Result<LedgerSnapshot, RuntimeError> {
        if input.len() > MAX_LEDGER_STATE_BYTES {
            return Err(RuntimeError::StateTooLarge);
        }
        let mut decoder = Decoder::new(input);
        if decoder.read_array::<16>()? != *STATE_DOMAIN {
            return Err(RuntimeError::InvalidDomain);
        }
        let rows = LedgerStateRows {
            network: NetworkId::new(decoder.read_array()?),
            registry_authority: ledger_core::AccountId::new(decoder.read_array()?),
            assets: decode_rows(&mut decoder)?,
            balances: decode_rows(&mut decoder)?,
            nonces: decode_rows(&mut decoder)?,
            operation_sequence: decode_rows(&mut decoder)?,
            account_statuses: decode_rows(&mut decoder)?,
        };
        decoder.finish()?;
        rows.into_snapshot(expected_network)
    }
}

fn encode_rows(output: &mut Vec<u8>, rows: &[LedgerRow]) -> Result<(), RuntimeError> {
    if rows.len() > MAX_STATE_ENTRIES {
        return Err(RuntimeError::TooManyEntries);
    }
    let count = u32::try_from(rows.len()).map_err(|_| RuntimeError::TooManyEntries)?;
    output.extend_from_slice(&count.to_be_bytes());
    for row in rows {
        if row.key.len() > MAX_ROW_KEY_BYTES || row.value.len() > MAX_ROW_VALUE_BYTES {
            return Err(RuntimeError::UnsupportedValue);
        }
        let key_length = u16::try_from(row.key.len()).map_err(|_| RuntimeError::LengthOverflow)?;
        let value_length =
            u16::try_from(row.value.len()).map_err(|_| RuntimeError::LengthOverflow)?;
        output.extend_from_slice(&key_length.to_be_bytes());
        output.extend_from_slice(&value_length.to_be_bytes());
        output.extend_from_slice(&row.key);
        output.extend_from_slice(&row.value);
    }
    Ok(())
}

fn decode_rows(decoder: &mut Decoder<'_>) -> Result<Vec<LedgerRow>, RuntimeError> {
    let count = usize::try_from(decoder.read_u32()?).map_err(|_| RuntimeError::TooManyEntries)?;
    if count > MAX_STATE_ENTRIES || count > decoder.remaining() / 4 {
        return Err(RuntimeError::TooManyEntries);
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let key_length = usize::from(decoder.read_u16()?);
        let value_length = usize::from(decoder.read_u16()?);
        if key_length > MAX_ROW_KEY_BYTES || value_length > MAX_ROW_VALUE_BYTES {
            return Err(RuntimeError::UnsupportedValue);
        }
        rows.push(LedgerRow {
            key: decoder.read_slice(key_length)?.to_vec(),
            value: decoder.read_slice(value_length)?.to_vec(),
        });
    }
    Ok(rows)
}
