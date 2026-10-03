use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, LedgerError, NetworkId,
    OperationKind, SignatureBytes, SignedOperation, Transfer, TransferBatch, TransferItem,
};
use transaction_auth_ed25519::Ed25519Verifier;

const REGISTRY: AccountId = AccountId::new([1; 32]);
const ISSUER: AccountId = AccountId::new([2; 32]);
const BACKING: AccountId = AccountId::new([3; 32]);
const FREEZER: AccountId = AccountId::new([4; 32]);
const TREASURY: AccountId = AccountId::new([5; 32]);
const RECIPIENT: AccountId = AccountId::new([6; 32]);
const TOKEN: AssetId = AssetId::new([11; 32]);
const KEY: IdempotencyKey = IdempotencyKey::new([21; 32]);
const NETWORK: NetworkId = NetworkId::new([42; 32]);
const OTHER_NETWORK: NetworkId = NetworkId::new([43; 32]);

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn account(key: &SigningKey) -> AccountId {
    AccountId::new(key.verifying_key().to_bytes())
}

fn funded_ledger(sender: AccountId, sponsor: AccountId) -> Ledger {
    let mut ledger = Ledger::new(NETWORK, REGISTRY);
    ledger
        .register_asset(
            REGISTRY,
            AssetDefinition {
                id: TOKEN,
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
    ledger.mint(ISSUER, TOKEN, sender, 1_000).unwrap();
    ledger.mint(ISSUER, TOKEN, sponsor, 100).unwrap();
    ledger
}

fn transfer(sender: AccountId) -> Transfer {
    Transfer {
        network: NETWORK,
        idempotency_key: KEY,
        asset: TOKEN,
        from: sender,
        to: RECIPIENT,
        amount: 100,
        fee: 5,
        nonce: 0,
        valid_until_height: 10_000,
    }
}

fn authorize(
    operation: &AuthorizedOperation,
    role: AuthorizationRole,
    key: &SigningKey,
) -> Authorization {
    let message = operation.authorization_message(role).unwrap();
    Authorization {
        signer: account(key),
        signature: SignatureBytes::new(key.sign(&message).to_bytes()),
    }
}

fn sender_signed(operation: AuthorizedOperation, key: &SigningKey) -> SignedOperation {
    let sender_authorization = authorize(&operation, AuthorizationRole::Sender, key);
    SignedOperation {
        operation,
        sender_authorization,
        fee_payer_authorization: None,
    }
}

#[test]
fn valid_signature_executes_transfer_and_returns_receipt() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let mut ledger = funded_ledger(sender, account(&sponsor_key));
    let operation = AuthorizedOperation::Transfer(transfer(sender));
    let expected_operation_id = operation.operation_id().unwrap();
    let signed = sender_signed(operation, &sender_key);

    let receipt = ledger.submit_signed(&Ed25519Verifier, signed).unwrap();

    assert_eq!(receipt.kind, OperationKind::Transfer);
    assert_eq!(receipt.operation_id, expected_operation_id);
    assert_eq!(ledger.balance(TOKEN, sender), 895);
    assert_eq!(ledger.balance(TOKEN, RECIPIENT), 100);
    assert_eq!(ledger.balance(TOKEN, TREASURY), 5);
}

#[test]
fn operation_id_is_deterministic_and_variant_sensitive() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let payment = transfer(sender);
    let ordinary = AuthorizedOperation::Transfer(payment);
    let sponsored = AuthorizedOperation::SponsoredTransfer {
        transfer: payment,
        fee_payer: account(&sponsor_key),
    };

    assert_eq!(ordinary.operation_id(), ordinary.operation_id());
    assert_ne!(ordinary.operation_id(), sponsored.operation_id());

    let mut changed = payment;
    changed.amount += 1;
    assert_ne!(
        ordinary.operation_id(),
        AuthorizedOperation::Transfer(changed).operation_id()
    );
}

#[test]
fn replaying_the_same_signed_operation_is_rejected() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let mut ledger = funded_ledger(sender, account(&sponsor_key));
    let signed = sender_signed(AuthorizedOperation::Transfer(transfer(sender)), &sender_key);

    ledger
        .submit_signed(&Ed25519Verifier, signed.clone())
        .unwrap();

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::NonceMismatch {
            expected: 1,
            actual: 0,
        })
    );
    assert_eq!(ledger.balance(TOKEN, sender), 895);
    assert_eq!(ledger.nonce(sender), 1);
}

#[test]
fn every_monetary_field_is_covered_by_the_signature() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let original = AuthorizedOperation::Transfer(transfer(sender));
    let authorization = authorize(&original, AuthorizationRole::Sender, &sender_key);
    let mut mutations = Vec::new();

    let mut changed = transfer(sender);
    changed.network = OTHER_NETWORK;
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.idempotency_key = IdempotencyKey::new([99; 32]);
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.asset = AssetId::new([99; 32]);
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.from = AccountId::new([98; 32]);
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.to = AccountId::new([99; 32]);
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.amount += 1;
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.fee += 1;
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.nonce += 1;
    mutations.push(changed);
    let mut changed = transfer(sender);
    changed.valid_until_height += 1;
    mutations.push(changed);

    for changed in mutations {
        let mut ledger = funded_ledger(sender, account(&sponsor_key));
        let expected = if changed.network == OTHER_NETWORK {
            LedgerError::WrongNetwork
        } else if changed.from != sender {
            LedgerError::InvalidAuthorizationSet
        } else {
            LedgerError::InvalidSignature
        };
        let signed = SignedOperation {
            operation: AuthorizedOperation::Transfer(changed),
            sender_authorization: authorization,
            fee_payer_authorization: None,
        };

        assert_eq!(
            ledger.submit_signed(&Ed25519Verifier, signed),
            Err(expected)
        );
        assert_eq!(ledger.balance(TOKEN, sender), 1_000);
        assert_eq!(ledger.nonce(sender), 0);
    }
}

#[test]
fn signer_must_be_the_declared_sender() {
    let sender_key = signing_key(7);
    let attacker_key = signing_key(9);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let operation = AuthorizedOperation::Transfer(transfer(sender));
    let signed = sender_signed(operation, &attacker_key);
    let mut ledger = funded_ledger(sender, account(&sponsor_key));

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidAuthorizationSet)
    );
    assert_eq!(ledger.balance(TOKEN, sender), 1_000);
}

#[test]
fn sponsored_transfer_requires_two_role_bound_signatures() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let sponsor = account(&sponsor_key);
    let operation = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(sender),
        fee_payer: sponsor,
    };
    let signed = SignedOperation {
        sender_authorization: authorize(&operation, AuthorizationRole::Sender, &sender_key),
        fee_payer_authorization: Some(authorize(
            &operation,
            AuthorizationRole::FeePayer,
            &sponsor_key,
        )),
        operation,
    };
    let mut ledger = funded_ledger(sender, sponsor);

    let receipt = ledger.submit_signed(&Ed25519Verifier, signed).unwrap();

    assert_eq!(receipt.kind, OperationKind::SponsoredTransfer);
    assert_eq!(ledger.balance(TOKEN, sender), 900);
    assert_eq!(ledger.balance(TOKEN, sponsor), 95);
}

#[test]
fn sponsored_operation_rejects_missing_fee_payer_authorization() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let sponsor = account(&sponsor_key);
    let operation = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(sender),
        fee_payer: sponsor,
    };
    let signed = sender_signed(operation, &sender_key);
    let mut ledger = funded_ledger(sender, sponsor);

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidAuthorizationSet)
    );
    assert_eq!(ledger.balance(TOKEN, sender), 1_000);
    assert_eq!(ledger.balance(TOKEN, sponsor), 100);
}

#[test]
fn changing_the_declared_fee_payer_invalidates_both_consents() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let replacement_key = signing_key(9);
    let sender = account(&sender_key);
    let sponsor = account(&sponsor_key);
    let operation = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(sender),
        fee_payer: sponsor,
    };
    let sender_authorization = authorize(&operation, AuthorizationRole::Sender, &sender_key);
    let changed = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(sender),
        fee_payer: account(&replacement_key),
    };
    let signed = SignedOperation {
        operation: changed,
        sender_authorization,
        fee_payer_authorization: Some(authorize(
            &operation,
            AuthorizationRole::FeePayer,
            &sponsor_key,
        )),
    };
    let mut ledger = funded_ledger(sender, sponsor);

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidAuthorizationSet)
    );
    assert_eq!(ledger.balance(TOKEN, sender), 1_000);
}

#[test]
fn sender_signature_cannot_be_reused_as_sponsor_consent() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let sponsor = account(&sponsor_key);
    let operation = AuthorizedOperation::SponsoredTransfer {
        transfer: transfer(sender),
        fee_payer: sponsor,
    };
    let sender_authorization = authorize(&operation, AuthorizationRole::Sender, &sender_key);
    let signed = SignedOperation {
        operation,
        sender_authorization,
        fee_payer_authorization: Some(Authorization {
            signer: sponsor,
            signature: sender_authorization.signature,
        }),
    };
    let mut ledger = funded_ledger(sender, sponsor);

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidSignature)
    );
    assert_eq!(ledger.balance(TOKEN, sender), 1_000);
    assert_eq!(ledger.balance(TOKEN, sponsor), 100);
}

#[test]
fn normal_operation_rejects_an_unexpected_sponsor_signature() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let operation = AuthorizedOperation::Transfer(transfer(sender));
    let mut signed = sender_signed(operation.clone(), &sender_key);
    signed.fee_payer_authorization = Some(authorize(
        &operation,
        AuthorizationRole::FeePayer,
        &sponsor_key,
    ));
    let mut ledger = funded_ledger(sender, account(&sponsor_key));

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidAuthorizationSet)
    );
}

#[test]
fn batch_item_order_and_values_are_covered_by_the_signature() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let batch = TransferBatch {
        network: NETWORK,
        idempotency_key: KEY,
        asset: TOKEN,
        from: sender,
        items: vec![
            TransferItem {
                to: RECIPIENT,
                amount: 40,
            },
            TransferItem {
                to: TREASURY,
                amount: 10,
            },
        ],
        fee: 2,
        nonce: 0,
        valid_until_height: 10_000,
    };
    let original = AuthorizedOperation::TransferBatch(batch.clone());
    let authorization = authorize(&original, AuthorizationRole::Sender, &sender_key);
    let mut reordered = batch.clone();
    reordered.items.swap(0, 1);
    let mut changed_value = batch;
    changed_value.items[0].amount += 1;

    for changed in [reordered, changed_value] {
        let signed = SignedOperation {
            operation: AuthorizedOperation::TransferBatch(changed),
            sender_authorization: authorization,
            fee_payer_authorization: None,
        };
        let mut ledger = funded_ledger(sender, account(&sponsor_key));

        assert_eq!(
            ledger.submit_signed(&Ed25519Verifier, signed),
            Err(LedgerError::InvalidSignature)
        );
        assert_eq!(ledger.balance(TOKEN, sender), 1_000);
    }
}

#[test]
fn sponsored_batch_dispatches_only_after_both_signatures_verify() {
    let sender_key = signing_key(7);
    let sponsor_key = signing_key(8);
    let sender = account(&sender_key);
    let sponsor = account(&sponsor_key);
    let operation = AuthorizedOperation::SponsoredBatchTransfer {
        batch: TransferBatch {
            network: NETWORK,
            idempotency_key: KEY,
            asset: TOKEN,
            from: sender,
            items: vec![
                TransferItem {
                    to: RECIPIENT,
                    amount: 40,
                },
                TransferItem {
                    to: TREASURY,
                    amount: 10,
                },
            ],
            fee: 2,
            nonce: 0,
            valid_until_height: 10_000,
        },
        fee_payer: sponsor,
    };
    let signed = SignedOperation {
        sender_authorization: authorize(&operation, AuthorizationRole::Sender, &sender_key),
        fee_payer_authorization: Some(authorize(
            &operation,
            AuthorizationRole::FeePayer,
            &sponsor_key,
        )),
        operation,
    };
    let mut ledger = funded_ledger(sender, sponsor);

    let receipt = ledger.submit_signed(&Ed25519Verifier, signed).unwrap();

    assert_eq!(receipt.kind, OperationKind::SponsoredBatchTransfer);
    assert_eq!(ledger.balance(TOKEN, sender), 950);
    assert_eq!(ledger.balance(TOKEN, sponsor), 98);
    assert_eq!(ledger.balance(TOKEN, RECIPIENT), 40);
    assert_eq!(ledger.balance(TOKEN, TREASURY), 12);
}

#[test]
fn invalid_public_key_encoding_is_rejected_without_state_changes() {
    let sponsor_key = signing_key(8);
    let sender = AccountId::new([0xff; 32]);
    let operation = AuthorizedOperation::Transfer(transfer(sender));
    let signed = SignedOperation {
        operation,
        sender_authorization: Authorization {
            signer: sender,
            signature: SignatureBytes::new([0; 64]),
        },
        fee_payer_authorization: None,
    };
    let mut ledger = funded_ledger(sender, account(&sponsor_key));

    assert_eq!(
        ledger.submit_signed(&Ed25519Verifier, signed),
        Err(LedgerError::InvalidSignature)
    );
    assert_eq!(ledger.balance(TOKEN, sender), 1_000);
}

#[test]
fn canonical_transfer_message_has_stable_field_boundaries() {
    let sender_key = signing_key(7);
    let sender = account(&sender_key);
    let payment = transfer(sender);
    let message = AuthorizedOperation::Transfer(payment)
        .authorization_message(AuthorizationRole::Sender)
        .unwrap();

    assert_eq!(message.len(), 231);
    assert_eq!(&message[..21], b"ledger.authorization\0");
    assert_eq!(message[21], 0);
    assert_eq!(message[22], 0);
    assert_eq!(&message[23..55], NETWORK.as_bytes());
    assert_eq!(&message[55..87], KEY.as_bytes());
    assert_eq!(&message[87..119], TOKEN.as_bytes());
    assert_eq!(&message[119..151], sender.as_bytes());
    assert_eq!(&message[151..183], RECIPIENT.as_bytes());
    assert_eq!(&message[183..199], &100_u128.to_be_bytes());
    assert_eq!(&message[199..215], &5_u128.to_be_bytes());
    assert_eq!(&message[215..223], &0_u64.to_be_bytes());
    assert_eq!(&message[223..231], &10_000_u64.to_be_bytes());
}

#[test]
fn oversized_batch_cannot_produce_an_authorization_message() {
    let sender_key = signing_key(7);
    let sender = account(&sender_key);
    let operation = AuthorizedOperation::TransferBatch(TransferBatch {
        network: NETWORK,
        idempotency_key: KEY,
        asset: TOKEN,
        from: sender,
        items: vec![
            TransferItem {
                to: RECIPIENT,
                amount: 1,
            };
            101
        ],
        fee: 1,
        nonce: 0,
        valid_until_height: 10_000,
    });

    assert_eq!(
        operation.authorization_message(AuthorizationRole::Sender),
        Err(LedgerError::BatchTooLarge)
    );
}
