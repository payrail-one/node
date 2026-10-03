#![forbid(unsafe_code)]

use bech32::{Bech32m, Hrp, primitives::decode::CheckedHrpstring};
use ledger_core::{AccountId, NetworkId};

pub const ACCOUNT_ADDRESS_TYPE: u8 = 0;
const ADDRESS_PAYLOAD_BYTES: usize = 33;
pub const MAX_ADDRESS_CHARS: usize = 90;
const MIN_HRP_CHARS: usize = 2;
const MAX_HRP_CHARS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressError {
    InvalidHumanReadablePrefix,
    InvalidEncoding,
    NonCanonicalCase,
    AddressTooLong,
    WrongNetwork,
    InvalidPayloadLength,
    UnsupportedAddressFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedAddress {
    pub network: NetworkId,
    pub account: AccountId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressCodec {
    network: NetworkId,
    hrp: Hrp,
}

impl AddressCodec {
    /// Creates an address codec for one explicitly configured network.
    ///
    /// Prefixes are deliberately configuration, not product constants, until
    /// final naming and network registration are approved.
    ///
    /// # Errors
    ///
    /// Returns an error unless the prefix is 2–16 lowercase ASCII
    /// alphanumeric characters and begins with a letter.
    pub fn new(network: NetworkId, hrp: &str) -> Result<Self, AddressError> {
        if !is_allowed_hrp(hrp) {
            return Err(AddressError::InvalidHumanReadablePrefix);
        }
        let hrp = Hrp::parse(hrp).map_err(|_| AddressError::InvalidHumanReadablePrefix)?;
        Ok(Self { network, hrp })
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub fn human_readable_prefix(&self) -> &str {
        self.hrp.as_str()
    }

    /// Encodes a canonical lowercase Bech32m address.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoder cannot produce a bounded address.
    pub fn encode(&self, account: AccountId) -> Result<String, AddressError> {
        let mut payload = [0_u8; ADDRESS_PAYLOAD_BYTES];
        payload[0] = ACCOUNT_ADDRESS_TYPE;
        payload[1..].copy_from_slice(account.as_bytes());
        let address = bech32::encode::<Bech32m>(self.hrp, &payload)
            .map_err(|_| AddressError::InvalidEncoding)?;
        if address.len() > MAX_ADDRESS_CHARS {
            return Err(AddressError::AddressTooLong);
        }
        Ok(address)
    }

    /// Decodes only canonical lowercase Bech32m addresses for this network.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, non-canonical, wrong-network,
    /// checksum-invalid or unsupported address data.
    pub fn decode(&self, address: &str) -> Result<DecodedAddress, AddressError> {
        if address.len() > MAX_ADDRESS_CHARS {
            return Err(AddressError::AddressTooLong);
        }
        if address.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(AddressError::NonCanonicalCase);
        }
        let checked =
            CheckedHrpstring::new::<Bech32m>(address).map_err(|_| AddressError::InvalidEncoding)?;
        if checked.hrp() != self.hrp {
            return Err(AddressError::WrongNetwork);
        }
        let payload = checked.byte_iter().collect::<Vec<_>>();
        if payload.len() != ADDRESS_PAYLOAD_BYTES {
            return Err(AddressError::InvalidPayloadLength);
        }
        if payload[0] != ACCOUNT_ADDRESS_TYPE {
            return Err(AddressError::UnsupportedAddressFormat);
        }
        let mut account = [0_u8; 32];
        account.copy_from_slice(&payload[1..]);
        Ok(DecodedAddress {
            network: self.network,
            account: AccountId::new(account),
        })
    }

    #[must_use]
    pub fn is_valid(&self, address: &str) -> bool {
        self.decode(address).is_ok()
    }
}

fn is_allowed_hrp(hrp: &str) -> bool {
    let bytes = hrp.as_bytes();
    (MIN_HRP_CHARS..=MAX_HRP_CHARS).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}
