use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, NetworkId, SignatureBytes,
    SignedOperation,
};
use payment_gateway_core::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentGateway, PaymentGatewayOutcome,
    PaymentOperationSigner, PaymentPolicy, PaymentPublisher, PublicationAck, SignedPaymentVerifier,
};
use payment_idempotency_core::{ClientRequestKey, PaymentRequestId, TenantId};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use payment_ingress_core::{
    ApiPrincipalId, BoundedPaymentRateLimiter, BucketPolicy, PaymentIngressError,
    PaymentIngressIdentity, PaymentIngressJournal, PaymentIngressService, PaymentRateLimitConfig,
    RateLimitError, RateLimitScope, TrackingCapacities,
};

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const ACCOUNT: AccountId = AccountId::new([2; 32]);
const TENANT: TenantId = TenantId::new([3; 32]);
const PRINCIPAL: ApiPrincipalId = ApiPrincipalId::new([4; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestServiceError {
    Unavailable,
}

struct TestServices {
    publications: Cell<u64>,
}

struct CountingJournal<'a> {
    inner: &'a LmdbPaymentIdempotencyStore,
    reads: Cell<u64>,
}

impl PaymentIngressJournal for CountingJournal<'_> {
    type Error = payment_idempotency_lmdb::PaymentIdempotencyStoreError;

    fn network(&self) -> NetworkId {
        self.inner.network()
    }

    fn get(
        &self,
        id: PaymentRequestId,
    ) -> Result<Option<payment_idempotency_core::PaymentReservation>, Self::Error> {
        self.reads.set(self.reads.get() + 1);
        self.inner.get(id)
    }
}

impl AccountNonceSource for TestServices {
    type Error = TestServiceError;

    fn finalized_next_nonce(
        &self,
        _network: NetworkId,
        _account: AccountId,
    ) -> Result<u64, Self::Error> {
        Ok(0)
    }
}

impl PaymentPolicy for TestServices {
    type Error = TestServiceError;

    fn authorize(&self, _request: &CanonicalPaymentRequest) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl PaymentOperationSigner for TestServices {
    type Error = TestServiceError;

    fn sign(&self, operation: AuthorizedOperation) -> Result<SignedOperation, Self::Error> {
        Ok(SignedOperation {
            sender_authorization: Authorization {
                signer: operation.sender(),
                signature: SignatureBytes::new([5; 64]),
            },
            fee_payer_authorization: operation.fee_payer().map(|signer| Authorization {
                signer,
                signature: SignatureBytes::new([6; 64]),
            }),
            operation,
        })
    }
}

impl SignedPaymentVerifier for TestServices {
    type Error = TestServiceError;

    fn verify(&self, _signed: &SignedOperation) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl PaymentPublisher for TestServices {
    type Error = TestServiceError;

    fn publish(&self, _signed: &SignedOperation) -> Result<PublicationAck, Self::Error> {
        self.publications.set(self.publications.get() + 1);
        Ok(PublicationAck::Accepted)
    }
}

fn directory() -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("payment-ingress-{}-{sequence}", std::process::id()))
}

fn request(client: u8, amount: u128) -> CanonicalPaymentRequest {
    CanonicalPaymentRequest {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TENANT,
            account: ACCOUNT,
            client_key: ClientRequestKey::new([client; 32]),
        },
        asset: AssetId::new([7; 32]),
        recipient: AccountId::new([8; 32]),
        amount,
        fee: 1,
        fee_payer: None,
        valid_until_height: 10_000,
    }
}

fn limiter() -> BoundedPaymentRateLimiter {
    let config = PaymentRateLimitConfig::new(
        BucketPolicy::new(4, 1_000).unwrap(),
        BucketPolicy::new(2, 1_000).unwrap(),
        BucketPolicy::new(1, 1_000).unwrap(),
        TrackingCapacities {
            principals: 8,
            tenants: 8,
            accounts: 8,
        },
        4_000,
    )
    .unwrap();
    BoundedPaymentRateLimiter::new(config)
}

fn identity() -> PaymentIngressIdentity {
    PaymentIngressIdentity {
        principal: PRINCIPAL,
        tenant: TENANT,
        account: ACCOUNT,
    }
}

#[test]
fn service_limits_new_intents_but_preserves_exact_gateway_retry() {
    let path = directory();
    let journal = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices {
        publications: Cell::new(0),
    };
    let gateway = PaymentGateway::new(
        &journal, &services, &services, &services, &services, &services,
    );
    let ingress_journal = CountingJournal {
        inner: &journal,
        reads: Cell::new(0),
    };
    let mut limiter = limiter();
    let mut ingress = PaymentIngressService::new(&ingress_journal, &mut limiter, &gateway);

    assert!(matches!(
        ingress.submit(identity(), request(9, 10), 0).unwrap(),
        PaymentGatewayOutcome::Submitted(_)
    ));
    assert_eq!(
        ingress.submit(identity(), request(10, 10), 0),
        Err(PaymentIngressError::RateLimited {
            scope: RateLimitScope::Account,
            retry_at_ms: 1_000,
        })
    );
    assert!(journal.get(request(10, 10).request_id).unwrap().is_none());

    // Exact recovery does not consume account/tenant quota again, but it is
    // still bounded by the authenticated API principal.
    assert!(matches!(
        ingress.submit(identity(), request(9, 10), 0).unwrap(),
        PaymentGatewayOutcome::Submitted(_)
    ));
    assert_eq!(
        ingress.submit(identity(), request(9, 11), 0),
        Err(PaymentIngressError::RequestConflict)
    );
    assert!(matches!(
        ingress.submit(identity(), request(9, 10), 0),
        Err(PaymentIngressError::RateLimited {
            scope: RateLimitScope::ApiPrincipal,
            retry_at_ms: 1_000,
        })
    ));
    assert_eq!(services.publications.get(), 2);
    assert_eq!(ingress_journal.reads.get(), 4);

    drop(journal);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn wrong_network_fails_before_quota_or_gateway_mutation() {
    let path = directory();
    let journal = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices {
        publications: Cell::new(0),
    };
    let gateway = PaymentGateway::new(
        &journal, &services, &services, &services, &services, &services,
    );
    let ingress_journal = CountingJournal {
        inner: &journal,
        reads: Cell::new(0),
    };
    let mut limiter = limiter();
    let mut ingress = PaymentIngressService::new(&ingress_journal, &mut limiter, &gateway);
    let mut wrong = request(9, 10);
    wrong.network = NetworkId::new([99; 32]);

    assert!(matches!(
        ingress.submit(identity(), wrong, 0),
        Err(PaymentIngressError::WrongNetwork)
    ));
    assert_eq!(services.publications.get(), 0);
    assert_eq!(ingress_journal.reads.get(), 0);
    assert_eq!(limiter.tracked_identities(RateLimitScope::ApiPrincipal), 0);

    drop(journal);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn unauthorized_tenant_or_account_context_fails_before_lookup_and_quota() {
    let path = directory();
    let journal = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices {
        publications: Cell::new(0),
    };
    let gateway = PaymentGateway::new(
        &journal, &services, &services, &services, &services, &services,
    );
    let ingress_journal = CountingJournal {
        inner: &journal,
        reads: Cell::new(0),
    };
    let mut limiter = limiter();
    let mut ingress = PaymentIngressService::new(&ingress_journal, &mut limiter, &gateway);
    let mut wrong_identity = identity();
    wrong_identity.tenant = TenantId::new([88; 32]);

    assert!(matches!(
        ingress.submit(wrong_identity, request(9, 10), 0),
        Err(PaymentIngressError::IdentityMismatch)
    ));
    let mut wrong_identity = identity();
    wrong_identity.account = AccountId::new([89; 32]);
    assert!(matches!(
        ingress.submit(wrong_identity, request(9, 10), 0),
        Err(PaymentIngressError::IdentityMismatch)
    ));
    assert_eq!(services.publications.get(), 0);
    assert_eq!(ingress_journal.reads.get(), 0);
    assert_eq!(limiter.tracked_identities(RateLimitScope::ApiPrincipal), 0);

    drop(journal);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn limiter_failures_remain_distinct_from_business_denials() {
    let error: PaymentIngressError<(), RateLimitError, TestServiceError> =
        PaymentIngressError::Limiter(RateLimitError::ClockRegression {
            previous_ms: 10,
            now_ms: 9,
        });
    assert_eq!(
        error,
        PaymentIngressError::Limiter(RateLimitError::ClockRegression {
            previous_ms: 10,
            now_ms: 9,
        })
    );
    let _ = TestServiceError::Unavailable;
}
