use ledger_core::{AccountId, AssetId, NetworkId};
use merchant_checkout_core::{
    Checkout, CheckoutDefinition, CheckoutDispatchState, CheckoutError, CheckoutMutationOutcome,
    CheckoutState, FeeMode, MerchantId, MerchantOrderKey,
};
use payment_idempotency_core::TenantId;

fn definition(fee_mode: FeeMode) -> CheckoutDefinition {
    CheckoutDefinition {
        network: NetworkId::new([1; 32]),
        tenant: TenantId::new([2; 32]),
        merchant: MerchantId::new([3; 32]),
        order_key: MerchantOrderKey::new([4; 32]),
        merchant_account: AccountId::new([5; 32]),
        asset: AssetId::new([6; 32]),
        amount: 100,
        maximum_fee: 5,
        fee_mode,
        expires_at_ms: 200,
        valid_until_height: 10_000,
    }
}

#[test]
fn exact_claim_retry_survives_expiry_but_conflicting_claim_does_not() {
    let mut checkout = Checkout::new(definition(FeeMode::CustomerPays), 100).unwrap();
    let payer = AccountId::new([7; 32]);
    assert_eq!(
        checkout.claim(payer, 2, 150),
        Ok(CheckoutMutationOutcome::Applied)
    );
    assert_eq!(
        checkout.claim(payer, 2, 250),
        Ok(CheckoutMutationOutcome::ExistingSame)
    );
    assert_eq!(
        checkout.claim(AccountId::new([8; 32]), 2, 251),
        Err(CheckoutError::ClaimConflict)
    );
    assert_eq!(
        checkout.claim(payer, 3, 252),
        Err(CheckoutError::ClaimConflict)
    );
    assert_eq!(
        checkout.mark_gateway_recorded(253),
        Ok(CheckoutMutationOutcome::Applied)
    );
    assert!(matches!(
        checkout.state(),
        CheckoutState::Claimed {
            dispatch: CheckoutDispatchState::GatewayRecorded,
            ..
        }
    ));
}

#[test]
fn expiry_amount_fee_and_payer_invariants_fail_before_claim_mutation() {
    let mut zero = definition(FeeMode::CustomerPays);
    zero.amount = 0;
    assert_eq!(Checkout::new(zero, 100), Err(CheckoutError::ZeroAmount));

    let mut invalid_validity = definition(FeeMode::CustomerPays);
    invalid_validity.valid_until_height = 0;
    assert_eq!(
        Checkout::new(invalid_validity, 100),
        Err(CheckoutError::InvalidValidity)
    );

    let mut sponsored = definition(FeeMode::MerchantSponsored);
    sponsored.maximum_fee = 0;
    assert_eq!(
        Checkout::new(sponsored, 100),
        Err(CheckoutError::InvalidFeePolicy)
    );
    assert_eq!(
        Checkout::new(definition(FeeMode::CustomerPays), 200),
        Err(CheckoutError::InvalidExpiry)
    );

    let mut checkout = Checkout::new(definition(FeeMode::MerchantSponsored), 100).unwrap();
    assert_eq!(
        checkout.claim(AccountId::new([5; 32]), 1, 150),
        Err(CheckoutError::PayerIsMerchant)
    );
    assert_eq!(
        checkout.claim(AccountId::new([7; 32]), 6, 150),
        Err(CheckoutError::FeeTooHigh)
    );
    assert_eq!(
        checkout.claim(AccountId::new([7; 32]), 0, 150),
        Err(CheckoutError::InvalidFeePolicy)
    );
    assert!(matches!(checkout.state(), CheckoutState::Open));
}

#[test]
fn checkout_identity_is_order_scoped_not_body_scoped() {
    let first = definition(FeeMode::CustomerPays);
    let mut changed = first;
    changed.amount += 1;
    assert_eq!(first.checkout_id(), changed.checkout_id());

    let mut another_order = first;
    another_order.order_key = MerchantOrderKey::new([9; 32]);
    assert_ne!(first.checkout_id(), another_order.checkout_id());
}
