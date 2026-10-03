use ledger_core::AccountId;
use payment_idempotency_core::TenantId;
use payment_ingress_core::{
    ApiPrincipalId, BoundedPaymentRateLimiter, BucketPolicy, PaymentIngressIdentity,
    PaymentIngressRateLimiter, PaymentRateLimitConfig, RateLimitConfigError, RateLimitDecision,
    RateLimitError, RateLimitScope, TrackingCapacities,
};

fn policy(capacity: u32) -> BucketPolicy {
    BucketPolicy::new(capacity, 100).unwrap()
}

fn config(
    principal: u32,
    tenant: u32,
    account: u32,
    tracking: TrackingCapacities,
) -> PaymentRateLimitConfig {
    PaymentRateLimitConfig::new(
        policy(principal),
        policy(tenant),
        policy(account),
        tracking,
        1_000,
    )
    .unwrap()
}

fn identity(principal: u8, tenant: u8, account: u8) -> PaymentIngressIdentity {
    PaymentIngressIdentity {
        principal: ApiPrincipalId::new([principal; 32]),
        tenant: TenantId::new([tenant; 32]),
        account: AccountId::new([account; 32]),
    }
}

#[test]
fn principal_preflight_and_new_identity_quotas_refill_independently() {
    let mut limiter = BoundedPaymentRateLimiter::new(config(
        2,
        1,
        1,
        TrackingCapacities {
            principals: 8,
            tenants: 8,
            accounts: 8,
        },
    ));
    let first = identity(1, 2, 3);
    assert_eq!(
        limiter.admit_principal(first.principal, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_new_intent(first.tenant, first.account, 0),
        Ok(RateLimitDecision::Admitted)
    );
    let limited_tenant = identity(1, 2, 4);
    assert_eq!(
        limiter.admit_principal(limited_tenant.principal, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_new_intent(limited_tenant.tenant, limited_tenant.account, 0),
        Ok(RateLimitDecision::Limited {
            scope: RateLimitScope::Tenant,
            retry_at_ms: 100,
        })
    );

    // The failed tenant decision did not create or charge the account bucket,
    // so an independent tenant/account pair can still enter.
    let independent = identity(9, 5, 6);
    assert_eq!(
        limiter.admit_new_intent(independent.tenant, independent.account, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(limiter.tracked_identities(RateLimitScope::Account), 2);
    assert_eq!(
        limiter.admit_principal(first.principal, 0),
        Ok(RateLimitDecision::Limited {
            scope: RateLimitScope::ApiPrincipal,
            retry_at_ms: 100,
        })
    );
    assert_eq!(
        limiter.admit_principal(first.principal, 100),
        Ok(RateLimitDecision::Admitted)
    );
}

#[test]
fn principal_preflight_does_not_create_tenant_or_account_state() {
    let mut limiter = BoundedPaymentRateLimiter::new(config(
        1,
        1,
        1,
        TrackingCapacities {
            principals: 2,
            tenants: 2,
            accounts: 2,
        },
    ));
    assert_eq!(
        limiter.admit_principal(identity(7, 8, 9).principal, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(limiter.tracked_identities(RateLimitScope::ApiPrincipal), 1);
    assert_eq!(limiter.tracked_identities(RateLimitScope::Tenant), 0);
    assert_eq!(limiter.tracked_identities(RateLimitScope::Account), 0);
}

#[test]
fn bounded_tracking_evicts_only_after_safe_full_recovery() {
    let config = PaymentRateLimitConfig::new(
        policy(1),
        policy(1),
        policy(1),
        TrackingCapacities {
            principals: 1,
            tenants: 1,
            accounts: 1,
        },
        100,
    )
    .unwrap();
    let mut limiter = BoundedPaymentRateLimiter::new(config);
    let first = identity(1, 1, 1);
    assert_eq!(
        limiter.admit_principal(first.principal, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_new_intent(first.tenant, first.account, 0),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_principal(identity(2, 2, 2).principal, 99),
        Err(RateLimitError::TrackingCapacityReached(
            RateLimitScope::ApiPrincipal
        ))
    );
    let second = identity(2, 2, 2);
    assert_eq!(
        limiter.admit_principal(second.principal, 100),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_new_intent(second.tenant, second.account, 100),
        Ok(RateLimitDecision::Admitted)
    );
    assert_eq!(
        limiter.admit_principal(second.principal, 99),
        Err(RateLimitError::ClockRegression {
            previous_ms: 100,
            now_ms: 99,
        })
    );
}

#[test]
fn invalid_configuration_and_timestamp_overflow_fail_closed() {
    assert_eq!(
        BucketPolicy::new(0, 1),
        Err(RateLimitConfigError::ZeroBucketCapacity)
    );
    assert_eq!(
        BucketPolicy::new(1, 0),
        Err(RateLimitConfigError::ZeroRefillInterval)
    );
    assert_eq!(
        BucketPolicy::new(u32::MAX, u64::MAX),
        Err(RateLimitConfigError::RecoveryWindowOverflow)
    );
    assert_eq!(
        PaymentRateLimitConfig::new(
            policy(1),
            policy(1),
            policy(1),
            TrackingCapacities {
                principals: 0,
                tenants: 1,
                accounts: 1,
            },
            100,
        ),
        Err(RateLimitConfigError::ZeroTrackingCapacity(
            RateLimitScope::ApiPrincipal
        ))
    );
    assert_eq!(
        PaymentRateLimitConfig::new(
            policy(2),
            policy(1),
            policy(1),
            TrackingCapacities {
                principals: 1,
                tenants: 1,
                accounts: 1,
            },
            199,
        ),
        Err(RateLimitConfigError::IdleTtlTooShort { minimum_ms: 200 })
    );

    let mut limiter = BoundedPaymentRateLimiter::new(config(
        1,
        1,
        1,
        TrackingCapacities {
            principals: 1,
            tenants: 1,
            accounts: 1,
        },
    ));
    assert_eq!(
        limiter.admit_principal(identity(1, 1, 1).principal, u64::MAX),
        Err(RateLimitError::TimeOverflow)
    );
    assert_eq!(limiter.tracked_identities(RateLimitScope::ApiPrincipal), 0);
    assert_eq!(limiter.tracked_identities(RateLimitScope::Tenant), 0);
    assert_eq!(limiter.tracked_identities(RateLimitScope::Account), 0);
}
