use account_address::{AddressCodec, AddressError, DecodedAddress};
use bech32::{Bech32, Bech32m, Hrp};
use ledger_core::{AccountId, NetworkId};

const MAIN_NETWORK: NetworkId = NetworkId::new([1; 32]);
const TEST_NETWORK: NetworkId = NetworkId::new([2; 32]);
const ACCOUNT: AccountId = AccountId::new([42; 32]);

fn main_codec() -> AddressCodec {
    AddressCodec::new(MAIN_NETWORK, "main").unwrap()
}

#[test]
fn address_round_trip_preserves_network_and_account() {
    let codec = main_codec();
    let address = codec.encode(ACCOUNT).unwrap();

    assert_eq!(
        codec.decode(&address),
        Ok(DecodedAddress {
            network: MAIN_NETWORK,
            account: ACCOUNT,
        })
    );
    assert!(codec.is_valid(&address));
    assert!(address.len() <= 90);
}

#[test]
fn published_golden_vector_is_stable() {
    let address = main_codec().encode(ACCOUNT).unwrap();

    assert_eq!(
        address,
        "main1qq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z58uvtja"
    );
}

#[test]
fn same_account_has_distinct_network_addresses() {
    let main = main_codec();
    let test = AddressCodec::new(TEST_NETWORK, "test").unwrap();
    let main_address = main.encode(ACCOUNT).unwrap();
    let test_address = test.encode(ACCOUNT).unwrap();

    assert_ne!(main_address, test_address);
    assert_eq!(main.decode(&test_address), Err(AddressError::WrongNetwork));
    assert_eq!(test.decode(&main_address), Err(AddressError::WrongNetwork));
}

#[test]
fn uppercase_and_mixed_case_are_not_canonical() {
    let codec = main_codec();
    let address = codec.encode(ACCOUNT).unwrap();
    let uppercase = address.to_ascii_uppercase();
    let mut mixed = address;
    mixed.replace_range(..1, "M");

    assert_eq!(
        codec.decode(&uppercase),
        Err(AddressError::NonCanonicalCase)
    );
    assert_eq!(codec.decode(&mixed), Err(AddressError::NonCanonicalCase));
}

#[test]
fn corrupted_checksum_is_rejected() {
    let codec = main_codec();
    let mut address = codec.encode(ACCOUNT).unwrap().into_bytes();
    let last = address.len() - 1;
    address[last] = if address[last] == b'q' { b'p' } else { b'q' };
    let corrupted = String::from_utf8(address).unwrap();

    assert_eq!(codec.decode(&corrupted), Err(AddressError::InvalidEncoding));
}

#[test]
fn legacy_bech32_checksum_is_rejected() {
    let codec = main_codec();
    let mut payload = [0_u8; 33];
    payload[1..].copy_from_slice(ACCOUNT.as_bytes());
    let legacy = bech32::encode::<Bech32>(Hrp::parse("main").unwrap(), &payload).unwrap();

    assert_eq!(codec.decode(&legacy), Err(AddressError::InvalidEncoding));
}

#[test]
fn unsupported_format_and_payload_lengths_are_rejected() {
    let codec = main_codec();
    let hrp = Hrp::parse("main").unwrap();
    let unsupported = bech32::encode::<Bech32m>(hrp, &[1_u8; 33]).unwrap();
    let short = bech32::encode::<Bech32m>(hrp, &[0_u8; 32]).unwrap();
    let long = bech32::encode::<Bech32m>(hrp, &[0_u8; 34]).unwrap();

    assert_eq!(
        codec.decode(&unsupported),
        Err(AddressError::UnsupportedAddressFormat)
    );
    assert_eq!(
        codec.decode(&short),
        Err(AddressError::InvalidPayloadLength)
    );
    assert_eq!(codec.decode(&long), Err(AddressError::InvalidPayloadLength));
}

#[test]
fn human_readable_prefix_policy_is_strict() {
    for invalid in [
        "",
        "a",
        "MAIN",
        "1main",
        "ma-in",
        "main_name",
        "abcdefghijklmnopq",
    ] {
        assert_eq!(
            AddressCodec::new(MAIN_NETWORK, invalid),
            Err(AddressError::InvalidHumanReadablePrefix)
        );
    }

    for valid in ["aa", "main", "test2", "abcdefghijklmnop"] {
        assert!(AddressCodec::new(MAIN_NETWORK, valid).is_ok());
    }
}

#[test]
fn oversized_input_is_rejected_before_decoding() {
    let oversized = "q".repeat(91);
    assert_eq!(
        main_codec().decode(&oversized),
        Err(AddressError::AddressTooLong)
    );
}

#[test]
fn representative_account_values_round_trip() {
    let codec = main_codec();
    for byte in [0_u8, 1, 127, 254, 255] {
        let account = AccountId::new([byte; 32]);
        let address = codec.encode(account).unwrap();
        assert_eq!(codec.decode(&address).unwrap().account, account);
    }
}
