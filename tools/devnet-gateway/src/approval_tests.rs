use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use checkout_approval_core::ApprovalCodeState;
use ed25519_dalek::{Signer, SigningKey};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

use crate::{DevnetError, approval::ApprovalRail, service::NETWORK};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const MERCHANT_TOKEN: &[u8] = b"merchant-test-token-with-at-least-32-bytes";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "devnet-approval-api-{}-{sequence}",
            std::process::id()
        )))
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn signed_code_is_opaque_claimable_and_bound_to_one_checkout() {
    let directory = TestDirectory::new();
    let rail = ApprovalRail::open(&directory.0, &[0x31; 32], MERCHANT_TOKEN).unwrap();
    let payer = SigningKey::from_bytes(&[0x41; 32]);
    let account = AccountId::new(payer.verifying_key().to_bytes());
    let now_ms = 1_000_000;
    let device = [0x51; 32];
    let nonce = [0x61; 32];
    let signature = payer
        .sign(&issue_message(NETWORK, account, device, now_ms, nonce))
        .to_bytes();

    let issued = rail
        .issue(account, device, now_ms, nonce, signature, now_ms)
        .unwrap();
    assert_eq!(issued.code.len(), 6);
    assert!(issued.code.bytes().all(|byte| byte.is_ascii_digit()));
    assert!(!issued.session_token.contains(&issued.code));
    assert!(matches!(
        rail.record_for_session(&issued.session_token, now_ms)
            .unwrap()
            .state(),
        ApprovalCodeState::Ready
    ));

    assert_eq!(
        rail.authorize_merchant(Some("Bearer incorrect-merchant-token-value")),
        Err(DevnetError::ApprovalUnauthorized)
    );
    rail.authorize_merchant(Some(&format!(
        "Bearer {}",
        String::from_utf8_lossy(MERCHANT_TOKEN)
    )))
    .unwrap();
    let checkout = CheckoutId::new([0x71; 32]);
    let claimed = rail.claim(&issued.code, checkout, now_ms + 1).unwrap();
    assert_eq!(claimed.account(), account);
    assert!(matches!(
        rail.record_for_session(&issued.session_token, now_ms + 1)
            .unwrap()
            .state(),
        ApprovalCodeState::Claimed(claim) if claim.checkout == checkout
    ));
    assert_eq!(
        rail.claim(&issued.code, CheckoutId::new([0x72; 32]), now_ms + 2),
        Err(DevnetError::ApprovalConflict)
    );
    rail.consume(checkout, account, now_ms + 3).unwrap();
    assert!(matches!(
        rail.record_for_session(&issued.session_token, now_ms + 3)
            .unwrap()
            .state(),
        ApprovalCodeState::Consumed { .. }
    ));
}

#[test]
fn bad_signature_and_expired_session_fail_closed() {
    let directory = TestDirectory::new();
    let rail = ApprovalRail::open(&directory.0, &[0x32; 32], MERCHANT_TOKEN).unwrap();
    let payer = SigningKey::from_bytes(&[0x42; 32]);
    let account = AccountId::new(payer.verifying_key().to_bytes());
    let now_ms = 2_000_000;
    assert_eq!(
        rail.issue(account, [1; 32], now_ms, [2; 32], [0; 64], now_ms),
        Err(DevnetError::ApprovalUnauthorized)
    );
    let device = [3; 32];
    let nonce = [4; 32];
    let signature = payer
        .sign(&issue_message(NETWORK, account, device, now_ms, nonce))
        .to_bytes();
    let issued = rail
        .issue(account, device, now_ms, nonce, signature, now_ms)
        .unwrap();
    let expires = issued.expires_at_ms.parse::<u64>().unwrap();
    assert_eq!(
        rail.record_for_session(&issued.session_token, expires),
        Err(DevnetError::ApprovalInvalid)
    );
}

fn issue_message(
    network: NetworkId,
    account: AccountId,
    device: [u8; 32],
    issued_at_ms: u64,
    nonce: [u8; 32],
) -> Vec<u8> {
    let mut message = b"payrail.approval.issue.v1\0".to_vec();
    message.extend_from_slice(network.as_bytes());
    message.extend_from_slice(account.as_bytes());
    message.extend_from_slice(&device);
    message.extend_from_slice(&issued_at_ms.to_be_bytes());
    message.extend_from_slice(&nonce);
    message
}
