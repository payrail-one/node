use std::{
    fmt::Write as _,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use account_address::AddressCodec;
use devnet_gateway::{DevnetError, DevnetService};
use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizationRole, AuthorizedOperation, IdempotencyKey,
    NetworkId, SignatureBytes, SignedOperation, Transfer,
};
use transaction_protocol::SignedOperationCodec;

const NETWORK: NetworkId = NetworkId::new([17; 32]);
const ASSET: AssetId = AssetId::new([34; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "devnet-gateway-{name}-{}-{sequence}",
            std::process::id()
        )))
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn address(key: &SigningKey) -> String {
    AddressCodec::new(NETWORK, "paydev")
        .unwrap()
        .encode(AccountId::new(key.verifying_key().to_bytes()))
        .unwrap()
}

fn signed_transfer(
    sender: &SigningKey,
    recipient: AccountId,
    nonce: u64,
    marker: u8,
    amount: u128,
) -> SignedOperation {
    signed_transfer_exact(sender, recipient, nonce, [marker; 32], amount, 20)
}

fn signed_transfer_exact(
    sender: &SigningKey,
    recipient: AccountId,
    nonce: u64,
    idempotency: [u8; 32],
    amount: u128,
    valid_until_height: u64,
) -> SignedOperation {
    let sender_account = AccountId::new(sender.verifying_key().to_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new(idempotency),
        asset: ASSET,
        from: sender_account,
        to: recipient,
        amount,
        fee: 0,
        nonce,
        valid_until_height,
    });
    let message = operation
        .authorization_message(AuthorizationRole::Sender)
        .unwrap();
    SignedOperation {
        operation,
        sender_authorization: Authorization {
            signer: sender_account,
            signature: SignatureBytes::new(sender.sign(&message).to_bytes()),
        },
        fee_payer_authorization: None,
    }
}

fn hex_32(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64);
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
    }
    output
}

fn envelope_hex(signed: &SignedOperation) -> String {
    let mut output = String::new();
    for byte in SignedOperationCodec::encode(signed).unwrap() {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

#[test]
fn faucet_and_browser_style_transfer_reach_the_independent_explorer_index() {
    let directory = TestDirectory::new("payment");
    let mut devnet = DevnetService::open(&directory.0).unwrap();
    let sender_key = SigningKey::from_bytes(&[91; 32]);
    let recipient_key = SigningKey::from_bytes(&[92; 32]);
    let sender_address = address(&sender_key);
    let recipient_address = address(&recipient_key);

    assert_eq!(devnet.account(&sender_address).unwrap().balance, "0");
    let faucet = devnet.faucet(&sender_address).unwrap();
    assert_eq!(faucet.transaction.outcome, "applied");
    assert_eq!(
        devnet.account(&sender_address).unwrap().balance,
        "100000000"
    );
    assert_eq!(
        devnet.faucet(&sender_address),
        Err(DevnetError::AccountAlreadyFunded)
    );

    let payment = signed_transfer(
        &sender_key,
        AccountId::new(recipient_key.verifying_key().to_bytes()),
        0,
        93,
        25_000_000,
    );
    let finalized = devnet.submit_hex(&envelope_hex(&payment)).unwrap();

    assert_eq!(finalized.transaction.from, sender_address);
    assert_eq!(finalized.transaction.to, recipient_address);
    assert_eq!(devnet.account(&sender_address).unwrap().balance, "75000000");
    assert_eq!(
        devnet.account(&recipient_address).unwrap().balance,
        "25000000"
    );
    let explorer = devnet.explorer();
    assert_eq!(explorer.status.finalized_height, "2");
    assert_eq!(explorer.blocks.len(), 3);
    assert_eq!(explorer.transactions.len(), 2);
    assert_eq!(explorer.transactions[0], finalized.transaction);
}

#[test]
fn rejected_signature_does_not_advance_ledger_or_explorer() {
    let directory = TestDirectory::new("bad-signature");
    let mut devnet = DevnetService::open(&directory.0).unwrap();
    let sender_key = SigningKey::from_bytes(&[94; 32]);
    let recipient_key = SigningKey::from_bytes(&[95; 32]);
    let sender_address = address(&sender_key);
    devnet.faucet(&sender_address).unwrap();
    let mut forged = signed_transfer(
        &sender_key,
        AccountId::new(recipient_key.verifying_key().to_bytes()),
        0,
        94,
        1,
    );
    forged.sender_authorization.signature = SignatureBytes::new([0; 64]);

    assert_eq!(
        devnet.submit_hex(&envelope_hex(&forged)),
        Err(DevnetError::InvalidTransaction)
    );
    assert_eq!(devnet.status().finalized_height, "1");
    assert_eq!(devnet.explorer().transactions.len(), 1);
    assert_eq!(devnet.account(&sender_address).unwrap().nonce, "0");
}

#[test]
fn finalized_state_faucet_history_and_explorer_recover_after_restart() {
    let directory = TestDirectory::new("restart");
    let sender_key = SigningKey::from_bytes(&[96; 32]);
    let recipient_key = SigningKey::from_bytes(&[97; 32]);
    let sender_address = address(&sender_key);
    let recipient = AccountId::new(recipient_key.verifying_key().to_bytes());

    let first_transaction = {
        let mut devnet = DevnetService::open(&directory.0).unwrap();
        devnet.faucet(&sender_address).unwrap();
        let payment = signed_transfer(&sender_key, recipient, 0, 98, 25_000_000);
        let finalized = devnet.submit_hex(&envelope_hex(&payment)).unwrap();
        assert_eq!(devnet.status().finalized_height, "2");
        finalized.transaction.id
    };

    let mut recovered = DevnetService::open(&directory.0).unwrap();
    assert_eq!(recovered.status().finalized_height, "2");
    assert_eq!(
        recovered.account(&sender_address).unwrap().balance,
        "75000000"
    );
    assert_eq!(
        recovered.faucet(&sender_address),
        Err(DevnetError::AccountAlreadyFunded)
    );
    assert!(
        recovered
            .explorer()
            .transactions
            .iter()
            .any(|transaction| transaction.id == first_transaction)
    );

    let second = signed_transfer(&sender_key, recipient, 1, 99, 5_000_000);
    recovered.submit_hex(&envelope_hex(&second)).unwrap();
    assert_eq!(recovered.status().finalized_height, "3");
    assert_eq!(
        recovered.account(&sender_address).unwrap().balance,
        "70000000"
    );
}

#[test]
fn replica_verifies_syncs_and_recovers_exported_finalized_blocks() {
    let source_directory = TestDirectory::new("replica-source");
    let replica_directory = TestDirectory::new("replica-target");
    let sender_key = SigningKey::from_bytes(&[101; 32]);
    let sender_address = address(&sender_key);
    let mut source = DevnetService::open(&source_directory.0).unwrap();
    source.faucet(&sender_address).unwrap();

    let bootstrap = source.sync_bootstrap().unwrap();
    assert_eq!(bootstrap.finalized_height, "1");
    let block = source.sync_block(1).unwrap();
    let mut replica = DevnetService::open(&replica_directory.0).unwrap();
    replica.apply_sync_block(&block).unwrap();
    assert_eq!(replica.finalized_height(), 1);
    assert_eq!(
        replica.account(&sender_address).unwrap().balance,
        "100000000"
    );

    drop(replica);
    let recovered = DevnetService::open(&replica_directory.0).unwrap();
    assert_eq!(recovered.finalized_height(), 1);
    assert_eq!(
        recovered.account(&sender_address).unwrap().balance,
        "100000000"
    );
}

#[test]
fn replica_rejects_tampered_block_without_advancing() {
    let source_directory = TestDirectory::new("tampered-source");
    let replica_directory = TestDirectory::new("tampered-target");
    let sender_key = SigningKey::from_bytes(&[102; 32]);
    let mut source = DevnetService::open(&source_directory.0).unwrap();
    source.faucet(&address(&sender_key)).unwrap();
    let mut block = source.sync_block(1).unwrap();
    block.checkpoint.state_root.replace_range(0..2, "00");

    let mut replica = DevnetService::open(&replica_directory.0).unwrap();
    assert_eq!(
        replica.apply_sync_block(&block),
        Err(DevnetError::InvalidSyncBlock)
    );
    assert_eq!(replica.finalized_height(), 0);
}

#[test]
fn replica_rejects_conflicting_parent_metadata_without_advancing() {
    let source_directory = TestDirectory::new("parent-source");
    let replica_directory = TestDirectory::new("parent-target");
    let sender_key = SigningKey::from_bytes(&[103; 32]);
    let mut source = DevnetService::open(&source_directory.0).unwrap();
    source.faucet(&address(&sender_key)).unwrap();
    let mut block = source.sync_block(1).unwrap();
    block.parent.state_root.replace_range(0..2, "00");

    let mut replica = DevnetService::open(&replica_directory.0).unwrap();
    assert_eq!(
        replica.apply_sync_block(&block),
        Err(DevnetError::InvalidSyncBlock)
    );
    assert_eq!(replica.finalized_height(), 0);
}

#[test]
fn fixed_checkout_finalizes_once_and_recovers_with_the_ledger() {
    let directory = TestDirectory::new("checkout");
    let payer_key = SigningKey::from_bytes(&[101; 32]);
    let merchant_key = SigningKey::from_bytes(&[102; 32]);
    let payer_address = address(&payer_key);
    let merchant_address = address(&merchant_key);

    let checkout_id = {
        let mut devnet = DevnetService::open(&directory.0).unwrap();
        devnet.faucet(&payer_address).unwrap();
        let checkout = devnet
            .create_checkout(&merchant_address, "12500000", "order_demo_1")
            .unwrap();
        assert_eq!(checkout.status, "open");
        assert!(checkout.sms_text.contains("12.5 TEST"));
        let payment = signed_transfer_exact(
            &payer_key,
            AccountId::new(merchant_key.verifying_key().to_bytes()),
            0,
            hex_32(&checkout.id),
            12_500_000,
            checkout.valid_until_height.parse().unwrap(),
        );
        let settled = devnet
            .submit_checkout(&checkout.id, &envelope_hex(&payment))
            .unwrap();
        assert_eq!(settled.status, "finalized");
        assert!(settled.transaction.is_some());
        assert_eq!(
            devnet.account(&merchant_address).unwrap().balance,
            "12500000"
        );
        checkout.id
    };

    let recovered = DevnetService::open(&directory.0).unwrap();
    let checkout = recovered.checkout(&checkout_id).unwrap();
    assert_eq!(checkout.status, "finalized");
    assert!(checkout.transaction.is_some());
    assert_eq!(
        recovered.account(&merchant_address).unwrap().balance,
        "12500000"
    );
}
