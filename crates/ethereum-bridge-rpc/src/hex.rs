use ethereum_bridge_core::{EvmAddress, EvmHash};

use crate::EvmRpcError;

pub(crate) fn decode_quantity(value: &str) -> Result<u64, EvmRpcError> {
    let digits = value
        .strip_prefix("0x")
        .ok_or(EvmRpcError::InvalidResponse)?;
    if digits.is_empty() || digits.len() > 16 || (digits.len() > 1 && digits.starts_with('0')) {
        return Err(EvmRpcError::InvalidResponse);
    }
    u64::from_str_radix(digits, 16).map_err(|_| EvmRpcError::InvalidResponse)
}

pub(crate) fn encode_quantity(value: u64) -> String {
    format!("0x{value:x}")
}

pub(crate) fn decode_hash(value: &str) -> Result<EvmHash, EvmRpcError> {
    let output = decode_fixed::<32>(value)?;
    let hash = EvmHash::new(output);
    if hash.is_zero() {
        return Err(EvmRpcError::InvalidResponse);
    }
    Ok(hash)
}

pub(crate) fn decode_address(value: &str) -> Result<EvmAddress, EvmRpcError> {
    Ok(EvmAddress::new(decode_fixed::<20>(value)?))
}

pub(crate) fn decode_bytes(value: &str, maximum: usize) -> Result<Vec<u8>, EvmRpcError> {
    let digits = value
        .strip_prefix("0x")
        .ok_or(EvmRpcError::InvalidResponse)?;
    if digits.len() % 2 != 0 || digits.len() / 2 > maximum {
        return Err(EvmRpcError::InvalidResponse);
    }
    digits
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| decode_pair(pair))
        .collect()
}

pub(crate) fn encode_address(value: EvmAddress) -> String {
    encode_bytes(value.as_bytes())
}

pub(crate) fn encode_hash(value: EvmHash) -> String {
    encode_bytes(value.as_bytes())
}

fn decode_fixed<const LENGTH: usize>(value: &str) -> Result<[u8; LENGTH], EvmRpcError> {
    let digits = value
        .strip_prefix("0x")
        .ok_or(EvmRpcError::InvalidResponse)?;
    if digits.len() != LENGTH.saturating_mul(2) {
        return Err(EvmRpcError::InvalidResponse);
    }
    let mut output = [0_u8; LENGTH];
    for (destination, pair) in output.iter_mut().zip(digits.as_bytes().as_chunks::<2>().0) {
        *destination = decode_pair(pair)?;
    }
    Ok(output)
}

fn decode_pair(pair: &[u8]) -> Result<u8, EvmRpcError> {
    let high = decode_nibble(pair[0])?;
    let low = decode_nibble(pair[1])?;
    Ok(high * 16 + low)
}

fn encode_bytes(value: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(2 + value.len().saturating_mul(2));
    encoded.push_str("0x");
    for byte in value {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn decode_nibble(value: u8) -> Result<u8, EvmRpcError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(EvmRpcError::InvalidResponse),
    }
}

#[cfg(test)]
mod tests {
    use ethereum_bridge_core::{EvmAddress, EvmHash};

    use super::{
        decode_address, decode_bytes, decode_hash, decode_quantity, encode_address, encode_hash,
        encode_quantity,
    };

    #[test]
    fn quantities_are_canonical_and_bounded() {
        assert_eq!(decode_quantity("0x0"), Ok(0));
        assert_eq!(decode_quantity("0xffff"), Ok(65_535));
        assert_eq!(encode_quantity(65_535), "0xffff");
        for invalid in ["", "0x", "0x00", "0X1", "0xg", "0x10000000000000000"] {
            assert!(decode_quantity(invalid).is_err());
        }
    }

    #[test]
    fn hashes_require_exact_nonzero_data() {
        let encoded = format!("0x{}", "ab".repeat(32));
        assert_eq!(decode_hash(&encoded).unwrap().as_bytes(), &[0xab; 32]);
        assert!(decode_hash(&format!("0x{}", "00".repeat(32))).is_err());
        assert!(decode_hash("0x12").is_err());
        assert!(decode_hash(&format!("0x{}", "zz".repeat(32))).is_err());
    }

    #[test]
    fn addresses_and_bounded_bytes_round_trip() {
        let address = EvmAddress::new([0xab; 20]);
        let hash = EvmHash::new([0xcd; 32]);
        assert_eq!(decode_address(&encode_address(address)), Ok(address));
        assert_eq!(decode_hash(&encode_hash(hash)), Ok(hash));
        assert_eq!(decode_bytes("0x00ab", 2), Ok(vec![0, 0xab]));
        assert!(decode_bytes("0x0", 2).is_err());
        assert!(decode_bytes("0x000000", 2).is_err());
    }
}
