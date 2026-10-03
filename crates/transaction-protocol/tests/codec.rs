use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, NetworkId, OperationId,
    SignatureBytes, SignedOperation, Transfer, TransferBatch, TransferItem,
};
use transaction_auth_ed25519::Ed25519Verifier;
use transaction_protocol::{MAX_ENVELOPE_BYTES, ProtocolError, SignedOperationCodec};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const ASSET: AssetId = AssetId::new([2; 32]);
const SENDER: AccountId = AccountId::new([3; 32]);
const RECIPIENT: AccountId = AccountId::new([4; 32]);
const SPONSOR: AccountId = AccountId::new([5; 32]);
const KEY: IdempotencyKey = IdempotencyKey::new([6; 32]);

fn transfer(sender: AccountId) -> Transfer {
    Transfer {
        network: NETWORK,
        idempotency_key: KEY,
        asset: ASSET,
        from: sender,
        to: RECIPIENT,
        amount: 500,
        fee: 7,
        nonce: 9,
        valid_until_height: 10_000,
    }
}

fn batch(sender: AccountId) -> TransferBatch {
    TransferBatch {
        network: NETWORK,
        idempotency_key: KEY,
        asset: ASSET,
        from: sender,
        items: vec![
            TransferItem {
                to: RECIPIENT,
                amount: 200,
            },
            TransferItem {
                to: SPONSOR,
                amount: 300,
            },
        ],
        fee: 7,
        nonce: 9,
        valid_until_height: 10_000,
    }
}

fn dummy_authorization(signer: AccountId, byte: u8) -> Authorization {
    Authorization {
        signer,
        signature: SignatureBytes::new([byte; 64]),
    }
}

fn envelope(operation: AuthorizedOperation) -> SignedOperation {
    let fee_payer_authorization = operation
        .fee_payer()
        .map(|fee_payer| dummy_authorization(fee_payer, 8));
    SignedOperation {
        operation,
        sender_authorization: dummy_authorization(SENDER, 7),
        fee_payer_authorization,
    }
}

fn all_operation_variants() -> Vec<SignedOperation> {
    vec![
        envelope(AuthorizedOperation::Transfer(transfer(SENDER))),
        envelope(AuthorizedOperation::TransferBatch(batch(SENDER))),
        envelope(AuthorizedOperation::SponsoredTransfer {
            transfer: transfer(SENDER),
            fee_payer: SPONSOR,
        }),
        envelope(AuthorizedOperation::SponsoredBatchTransfer {
            batch: batch(SENDER),
            fee_payer: SPONSOR,
        }),
    ]
}

#[test]
fn all_operation_variants_round_trip_canonically() {
    for signed in all_operation_variants() {
        let encoded = SignedOperationCodec::encode(&signed).unwrap();
        let decoded = SignedOperationCodec::decode(&encoded).unwrap();

        assert_eq!(decoded, signed);
        assert_eq!(SignedOperationCodec::encode(&decoded).unwrap(), encoded);
        assert_eq!(
            decoded.operation.operation_id(),
            signed.operation.operation_id()
        );
    }
}

#[test]
fn operation_id_has_a_stable_golden_value() {
    let expected = OperationId::new([
        0x9a, 0x67, 0xf0, 0x4d, 0x0e, 0x68, 0x48, 0xa4, 0x47, 0x0a, 0x9e, 0x0c, 0x76, 0x78, 0x73,
        0xac, 0x73, 0xa9, 0x50, 0xb4, 0x01, 0x90, 0xb4, 0x9f, 0x3b, 0xe1, 0x92, 0x25, 0xae, 0x30,
        0x01, 0xea,
    ]);

    assert_eq!(
        AuthorizedOperation::Transfer(transfer(SENDER)).operation_id(),
        Ok(expected)
    );
}

#[test]
fn ordinary_transfer_envelope_has_stable_header_and_length() {
    let encoded = SignedOperationCodec::encode(&all_operation_variants()[0]).unwrap();

    assert_eq!(encoded.len(), 322);
    assert_eq!(&encoded[..16], b"ledger.envelope\0");
    assert_eq!(encoded[16], 0);
}

#[test]
fn every_truncated_prefix_is_rejected() {
    for signed in all_operation_variants() {
        let encoded = SignedOperationCodec::encode(&signed).unwrap();
        for length in 0..encoded.len() {
            assert!(SignedOperationCodec::decode(&encoded[..length]).is_err());
        }
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut encoded = SignedOperationCodec::encode(&all_operation_variants()[0]).unwrap();
    encoded.push(0);

    assert_eq!(
        SignedOperationCodec::decode(&encoded),
        Err(ProtocolError::TrailingBytes)
    );
}

#[test]
fn invalid_domain_operation_and_authorization_flag_are_rejected() {
    let encoded = SignedOperationCodec::encode(&all_operation_variants()[0]).unwrap();

    let mut invalid_domain = encoded.clone();
    invalid_domain[0] ^= 1;
    assert_eq!(
        SignedOperationCodec::decode(&invalid_domain),
        Err(ProtocolError::InvalidDomain)
    );

    let mut invalid_operation = encoded.clone();
    invalid_operation[16] = u8::MAX;
    assert_eq!(
        SignedOperationCodec::decode(&invalid_operation),
        Err(ProtocolError::UnsupportedOperation)
    );

    let mut invalid_flag = encoded;
    let last = invalid_flag.len() - 1;
    invalid_flag[last] = 2;
    assert_eq!(
        SignedOperationCodec::decode(&invalid_flag),
        Err(ProtocolError::InvalidAuthorizationFlag)
    );
}

#[test]
fn authorization_count_must_match_the_operation_variant() {
    let operation = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(SENDER),
        fee_payer: SPONSOR,
    };
    let missing = SignedOperation {
        operation,
        sender_authorization: dummy_authorization(SENDER, 7),
        fee_payer_authorization: None,
    };
    assert_eq!(
        SignedOperationCodec::encode(&missing),
        Err(ProtocolError::AuthorizationSetMismatch)
    );

    let mut ordinary = SignedOperationCodec::encode(&all_operation_variants()[0]).unwrap();
    let last = ordinary.len() - 1;
    ordinary[last] = 1;
    ordinary.extend_from_slice(SPONSOR.as_bytes());
    ordinary.extend_from_slice(&[8; 64]);
    assert_eq!(
        SignedOperationCodec::decode(&ordinary),
        Err(ProtocolError::AuthorizationSetMismatch)
    );
}

#[test]
fn declared_batch_count_is_bounded_before_allocation() {
    let signed = envelope(AuthorizedOperation::TransferBatch(batch(SENDER)));
    let mut encoded = SignedOperationCodec::encode(&signed).unwrap();
    encoded[145..149].copy_from_slice(&101_u32.to_be_bytes());

    assert_eq!(
        SignedOperationCodec::decode(&encoded),
        Err(ProtocolError::BatchTooLarge)
    );
}

#[test]
fn envelope_size_is_bounded_before_parsing() {
    let oversized = vec![0_u8; MAX_ENVELOPE_BYTES + 1];
    assert_eq!(
        SignedOperationCodec::decode(&oversized),
        Err(ProtocolError::EnvelopeTooLarge)
    );
}

#[test]
fn decoded_real_signature_executes_end_to_end() {
    let sender_key = SigningKey::from_bytes(&[7; 32]);
    let sender = AccountId::new(sender_key.verifying_key().to_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: KEY,
        asset: ASSET,
        from: sender,
        to: RECIPIENT,
        amount: 100,
        fee: 5,
        nonce: 0,
        valid_until_height: 10_000,
    });
    let message = operation
        .authorization_message(AuthorizationRole::Sender)
        .unwrap();
    let expected_operation_id = operation.operation_id().unwrap();
    let signed = SignedOperation {
        operation,
        sender_authorization: Authorization {
            signer: sender,
            signature: SignatureBytes::new(sender_key.sign(&message).to_bytes()),
        },
        fee_payer_authorization: None,
    };
    let encoded = SignedOperationCodec::encode(&signed).unwrap();
    let decoded = SignedOperationCodec::decode(&encoded).unwrap();
    let mut ledger = funded_ledger(sender);

    let receipt = ledger.submit_signed(&Ed25519Verifier, decoded).unwrap();

    assert_eq!(receipt.operation_id, expected_operation_id);
    assert_eq!(ledger.balance(ASSET, sender), 895);
    assert_eq!(ledger.balance(ASSET, RECIPIENT), 100);
}

fn funded_ledger(sender: AccountId) -> Ledger {
    const REGISTRY: AccountId = AccountId::new([10; 32]);
    const ISSUER: AccountId = AccountId::new([11; 32]);
    const BACKING: AccountId = AccountId::new([12; 32]);
    const FREEZER: AccountId = AccountId::new([13; 32]);
    const TREASURY: AccountId = AccountId::new([14; 32]);

    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: ASSET,
                symbol: "PAY".to_owned(),
                decimals: 6,
                class: AssetClass::NetworkNative,
                status: AssetStatus::Active,
                issuer: ISSUER,
                backing_authority: BACKING,
                freeze_authority: FREEZER,
                treasury: TREASURY,
                max_supply: None,
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, ASSET, sender, 1_000).unwrap();
    ledger
}
