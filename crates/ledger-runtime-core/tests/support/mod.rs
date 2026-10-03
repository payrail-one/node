use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{
    AccountId, AssetClass, AssetDefinition, AssetId, AssetStatus, Authorization, AuthorizationRole,
    AuthorizedOperation, BackingRequirement, IdempotencyKey, Ledger, NetworkId, SignatureBytes,
    SignedOperation, Transfer,
};
use ledger_runtime_core::{BlockCandidate, LedgerStateCodec, state_root};
use state_sync_core::{BlockHash, FinalizedCheckpoint, ValidatorSetHash};
use transaction_protocol::SignedOperationCodec;

pub(crate) const NETWORK: NetworkId = NetworkId::new([1; 32]);
pub(crate) const TOKEN: AssetId = AssetId::new([2; 32]);
pub(crate) const REGISTRY: AccountId = AccountId::new([3; 32]);
pub(crate) const ISSUER: AccountId = AccountId::new([4; 32]);
pub(crate) const BACKING: AccountId = AccountId::new([5; 32]);
pub(crate) const FREEZER: AccountId = AccountId::new([6; 32]);
pub(crate) const TREASURY: AccountId = AccountId::new([7; 32]);
pub(crate) const RECIPIENT: AccountId = AccountId::new([8; 32]);

pub(crate) fn sender_key() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}

pub(crate) fn funded_ledger(sender: AccountId) -> Ledger {
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
                max_supply: Some(1_000_000),
                backing_requirement: BackingRequirement::None,
            },
        )
        .unwrap();
    ledger.mint(ISSUER, TOKEN, sender, 10_000).unwrap();
    ledger
}

pub(crate) fn signed_payment_until(
    key: &SigningKey,
    recipient: AccountId,
    nonce: u64,
    idempotency_byte: u8,
    amount: u128,
    fee: u128,
    valid_until_height: u64,
) -> SignedOperation {
    let sender = AccountId::new(key.verifying_key().to_bytes());
    let operation = AuthorizedOperation::Transfer(Transfer {
        network: NETWORK,
        idempotency_key: IdempotencyKey::new([idempotency_byte; 32]),
        asset: TOKEN,
        from: sender,
        to: recipient,
        amount,
        fee,
        nonce,
        valid_until_height,
    });
    let message = operation
        .authorization_message(AuthorizationRole::Sender)
        .unwrap();
    SignedOperation {
        operation,
        sender_authorization: Authorization {
            signer: sender,
            signature: SignatureBytes::new(key.sign(&message).to_bytes()),
        },
        fee_payer_authorization: None,
    }
}

pub(crate) fn initial_state() -> (AccountId, Vec<u8>, FinalizedCheckpoint) {
    let sender = AccountId::new(sender_key().verifying_key().to_bytes());
    let state = LedgerStateCodec::encode(&funded_ledger(sender).snapshot()).unwrap();
    let checkpoint = FinalizedCheckpoint {
        height: 100,
        block_hash: BlockHash::new([9; 32]),
        state_root: state_root(&state),
        validator_set_hash: ValidatorSetHash::new([10; 32]),
    };
    (sender, state, checkpoint)
}

pub(crate) fn candidates(operations: Vec<SignedOperation>) -> Vec<BlockCandidate> {
    let mut candidates = operations
        .into_iter()
        .map(|signed| BlockCandidate {
            operation: signed.operation.operation_id().unwrap(),
            envelope: SignedOperationCodec::encode(&signed).unwrap(),
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.operation);
    candidates
}
