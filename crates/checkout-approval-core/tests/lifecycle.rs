use checkout_approval_core::{
    ApprovalCode, ApprovalCodeDigest, ApprovalCodeError, ApprovalCodeMutationOutcome,
    ApprovalCodePolicy, ApprovalCodeRecord, ApprovalCodeState,
};
use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

fn record() -> ApprovalCodeRecord {
    ApprovalCodeRecord::issue(
        NetworkId::new([1; 32]),
        ApprovalCodeDigest::new([2; 32]),
        AccountId::new([3; 32]),
        ApprovalCodePolicy::TWO_MINUTES,
        1_000,
    )
    .unwrap()
}

#[test]
fn codes_are_strict_and_numeric_values_preserve_leading_zeroes() {
    assert_eq!(ApprovalCode::parse("000042").unwrap().expose(), "000042");
    assert_eq!(ApprovalCode::from_number(42).unwrap().expose(), "000042");
    assert_eq!(
        ApprovalCode::from_number(999_999).unwrap().expose(),
        "999999"
    );
    assert!(matches!(
        ApprovalCode::parse("12345"),
        Err(ApprovalCodeError::InvalidCode)
    ));
    assert!(matches!(
        ApprovalCode::parse("12 456"),
        Err(ApprovalCodeError::InvalidCode)
    ));
    assert!(matches!(
        ApprovalCode::parse("１２３４５６"),
        Err(ApprovalCodeError::InvalidCode)
    ));
    assert!(matches!(
        ApprovalCode::from_number(1_000_000),
        Err(ApprovalCodeError::InvalidCode)
    ));
}

#[test]
fn claim_is_single_checkout_and_expiry_is_exclusive() {
    let checkout = CheckoutId::new([4; 32]);
    let mut approval = record();
    assert_eq!(
        approval.claim(checkout, 120_999).unwrap(),
        ApprovalCodeMutationOutcome::Applied
    );
    assert_eq!(
        approval.claim(checkout, 120_999).unwrap(),
        ApprovalCodeMutationOutcome::ExistingSame
    );
    assert_eq!(
        approval.claim(CheckoutId::new([5; 32]), 120_999),
        Err(ApprovalCodeError::AlreadyClaimed)
    );

    let mut expired = record();
    assert_eq!(
        expired.claim(checkout, 121_000),
        Err(ApprovalCodeError::Expired)
    );
}

#[test]
fn only_a_claimed_checkout_can_be_consumed() {
    let checkout = CheckoutId::new([4; 32]);
    let mut approval = record();
    assert_eq!(
        approval.consume(checkout, 2_000),
        Err(ApprovalCodeError::InvalidTransition)
    );
    approval.claim(checkout, 2_000).unwrap();
    assert_eq!(
        approval.consume(checkout, 121_001).unwrap(),
        ApprovalCodeMutationOutcome::Applied
    );
    assert!(matches!(
        approval.state(),
        ApprovalCodeState::Consumed { .. }
    ));
    assert_eq!(
        approval.consume(checkout, 121_001).unwrap(),
        ApprovalCodeMutationOutcome::ExistingSame
    );
}

#[test]
fn restoration_rejects_impossible_timestamps() {
    assert_eq!(
        ApprovalCodeRecord::restore(
            NetworkId::new([1; 32]),
            ApprovalCodeDigest::new([2; 32]),
            AccountId::new([3; 32]),
            2_000,
            2_000,
            ApprovalCodeState::Ready,
        ),
        Err(ApprovalCodeError::CorruptState)
    );
}
