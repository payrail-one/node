use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{AccountId, SignatureBytes, SignatureVerification, SignatureVerifier};
use transaction_auth_ed25519::Ed25519Verifier;

#[test]
fn strict_batch_accepts_valid_distinct_messages() {
    let key = SigningKey::from_bytes(&[71; 32]);
    let signer = AccountId::new(key.verifying_key().to_bytes());
    let first = b"first authorization";
    let second = b"second authorization";
    let first_signature = SignatureBytes::new(key.sign(first).to_bytes());
    let second_signature = SignatureBytes::new(key.sign(second).to_bytes());
    let verifications = [
        SignatureVerification::new(signer, first, first_signature),
        SignatureVerification::new(signer, second, second_signature),
    ];

    assert!(Ed25519Verifier.verify_batch(&verifications));
}

#[test]
fn strict_batch_rejects_tampering_and_weak_keys() {
    let key = SigningKey::from_bytes(&[72; 32]);
    let signer = AccountId::new(key.verifying_key().to_bytes());
    let message = b"authorized payload";
    let signature = SignatureBytes::new(key.sign(message).to_bytes());
    let tampered = [
        SignatureVerification::new(signer, message, signature),
        SignatureVerification::new(signer, b"changed payload", signature),
    ];
    assert!(!Ed25519Verifier.verify_batch(&tampered));

    let mut identity = [0_u8; 32];
    identity[0] = 1;
    let weak = [
        SignatureVerification::new(signer, message, signature),
        SignatureVerification::new(
            AccountId::new(identity),
            message,
            SignatureBytes::new([0; 64]),
        ),
    ];
    assert!(!Ed25519Verifier.verify_batch(&weak));
}
