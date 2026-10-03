use ledger_core::{LedgerSnapshot, NetworkId};

use crate::{LedgerRow, LedgerStateRows, RuntimeError, decoder::Decoder};

const STATE_DOMAIN_V2: &[u8; 16] = b"ledger.state.v2\0";
const STATE_DOMAIN_V3: &[u8; 16] = b"ledger.state.v3\0";
const MAX_STATE_ENTRIES: usize = 1_000_000;
const MAX_ROW_KEY_BYTES: usize = 64;
const MAX_ROW_VALUE_BYTES: usize = 20 * 1024;
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
        let includes_contract_state = !rows.contracts.is_empty() || !rows.contract_state.is_empty();
        output.extend_from_slice(if includes_contract_state {
            STATE_DOMAIN_V3
        } else {
            STATE_DOMAIN_V2
        });
        output.extend_from_slice(rows.network.as_bytes());
        output.extend_from_slice(rows.registry_authority.as_bytes());
        encode_rows(&mut output, &rows.assets)?;
        encode_rows(&mut output, &rows.balances)?;
        encode_rows(&mut output, &rows.nonces)?;
        encode_rows(&mut output, &rows.operation_sequence)?;
        encode_rows(&mut output, &rows.account_statuses)?;
        if includes_contract_state {
            encode_rows(&mut output, &rows.contracts)?;
            encode_rows(&mut output, &rows.contract_state)?;
        }
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
        let domain = decoder.read_array::<16>()?;
        if domain != *STATE_DOMAIN_V2 && domain != *STATE_DOMAIN_V3 {
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
            contracts: if domain == *STATE_DOMAIN_V3 {
                decode_rows(&mut decoder)?
            } else {
                Vec::new()
            },
            contract_state: if domain == *STATE_DOMAIN_V3 {
                decode_rows(&mut decoder)?
            } else {
                Vec::new()
            },
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
