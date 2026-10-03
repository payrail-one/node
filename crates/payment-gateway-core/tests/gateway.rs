use std::{
    cell::{Cell, RefCell},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, NetworkId, SignatureBytes,
    SignedOperation,
};
use payment_gateway_core::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentGateway, PaymentGatewayError,
    PaymentGatewayOutcome, PaymentOperationSigner, PaymentPolicy, PaymentPublisher,
    PaymentServiceStage, PublicationAck, SignedPaymentVerifier,
};
use payment_idempotency_core::{ClientRequestKey, PaymentRequestId, ReservationState, TenantId};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use transaction_protocol::SignedOperationCodec;

const NETWORK: NetworkId = NetworkId::new([1; 32]);
const ACCOUNT: AccountId = AccountId::new([2; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestError {
    Denied,
    SignerUnavailable,
    PublishUnknown,
}

struct TestServices {
    policy_allowed: Cell<bool>,
    signer_available: Cell<bool>,
    publisher_available: Cell<bool>,
    signer_mutates_operation: Cell<bool>,
    nonce_calls: Cell<u64>,
    policy_calls: Cell<u64>,
    signer_calls: Cell<u64>,
    verifier_calls: Cell<u64>,
    published: RefCell<Vec<Vec<u8>>>,
}

impl TestServices {
    fn available() -> Self {
        Self {
            policy_allowed: Cell::new(true),
            signer_available: Cell::new(true),
            publisher_available: Cell::new(true),
            signer_mutates_operation: Cell::new(false),
            nonce_calls: Cell::new(0),
            policy_calls: Cell::new(0),
            signer_calls: Cell::new(0),
            verifier_calls: Cell::new(0),
            published: RefCell::new(Vec::new()),
        }
    }
}

impl AccountNonceSource for TestServices {
    type Error = TestError;

    fn finalized_next_nonce(
        &self,
        _network: NetworkId,
        _account: AccountId,
    ) -> Result<u64, Self::Error> {
        self.nonce_calls.set(self.nonce_calls.get() + 1);
        Ok(7)
    }
}

impl PaymentPolicy for TestServices {
    type Error = TestError;

    fn authorize(&self, _request: &CanonicalPaymentRequest) -> Result<(), Self::Error> {
        self.policy_calls.set(self.policy_calls.get() + 1);
        if self.policy_allowed.get() {
            Ok(())
        } else {
            Err(TestError::Denied)
        }
    }
}

impl PaymentOperationSigner for TestServices {
    type Error = TestError;

    fn sign(&self, mut operation: AuthorizedOperation) -> Result<SignedOperation, Self::Error> {
        self.signer_calls.set(self.signer_calls.get() + 1);
        if !self.signer_available.get() {
            return Err(TestError::SignerUnavailable);
        }
        if self.signer_mutates_operation.get() {
            match &mut operation {
                AuthorizedOperation::Transfer(transfer)
                | AuthorizedOperation::SponsoredTransfer { transfer, .. } => {
                    transfer.amount += 1;
                }
                AuthorizedOperation::TransferBatch(_)
                | AuthorizedOperation::SponsoredBatchTransfer { .. } => {}
            }
        }
        let sender = operation.sender();
        let fee_payer_authorization = operation.fee_payer().map(|fee_payer| Authorization {
            signer: fee_payer,
            signature: SignatureBytes::new([4; 64]),
        });
        Ok(SignedOperation {
            operation,
            sender_authorization: Authorization {
                signer: sender,
                signature: SignatureBytes::new([3; 64]),
            },
            fee_payer_authorization,
        })
    }
}

impl SignedPaymentVerifier for TestServices {
    type Error = TestError;

    fn verify(&self, _signed: &SignedOperation) -> Result<(), Self::Error> {
        self.verifier_calls.set(self.verifier_calls.get() + 1);
        Ok(())
    }
}

impl PaymentPublisher for TestServices {
    type Error = TestError;

    fn publish(&self, signed: &SignedOperation) -> Result<PublicationAck, Self::Error> {
        self.published
            .borrow_mut()
            .push(SignedOperationCodec::encode(signed).unwrap());
        if self.publisher_available.get() {
            Ok(PublicationAck::Accepted)
        } else {
            Err(TestError::PublishUnknown)
        }
    }
}

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "payment-gateway-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn request(amount: u128) -> CanonicalPaymentRequest {
    CanonicalPaymentRequest {
        network: NETWORK,
        request_id: PaymentRequestId {
            tenant: TenantId::new([5; 32]),
            account: ACCOUNT,
            client_key: ClientRequestKey::new([6; 32]),
        },
        asset: AssetId::new([7; 32]),
        recipient: AccountId::new([8; 32]),
        amount,
        fee: 1,
        fee_payer: None,
        valid_until_height: 10_000,
    }
}

#[test]
fn canonical_request_and_ledger_correlation_vectors_are_stable() {
    let payment = request(50);
    assert_eq!(
        payment.request_digest().as_bytes(),
        &[
            0xec, 0xff, 0x0f, 0xeb, 0xfb, 0x0f, 0x3e, 0xd9, 0x73, 0x10, 0xb1, 0x5e, 0xeb, 0x7a,
            0xf6, 0xba, 0xd3, 0x10, 0x53, 0x57, 0x4b, 0x2f, 0xd6, 0xde, 0x29, 0x31, 0x20, 0xe1,
            0xb6, 0x19, 0xdc, 0x12,
        ]
    );
    assert_eq!(
        payment.intent().unwrap().ledger_idempotency_key.as_bytes(),
        &[
            0x7d, 0xbb, 0x18, 0x8d, 0x6c, 0xd4, 0x52, 0x2f, 0xd6, 0x78, 0x2a, 0x11, 0x18, 0x96,
            0x19, 0x68, 0xb8, 0xd4, 0x11, 0xaa, 0x01, 0x46, 0x37, 0x44, 0x2a, 0x40, 0xaa, 0xb5,
            0x49, 0xff, 0x3b, 0x3c,
        ]
    );
}

#[test]
fn zero_consensus_validity_fails_before_journal_mutation() {
    let path = directory("invalid-validity");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::available();
    let gateway = PaymentGateway::new(
        &store, &services, &services, &services, &services, &services,
    );
    let mut invalid = request(50);
    invalid.valid_until_height = 0;

    assert_eq!(
        gateway.submit(invalid, 100),
        Err(PaymentGatewayError::InvalidRequest(
            payment_gateway_core::PaymentRequestError::InvalidValidity
        ))
    );
    assert!(store.get(invalid.request_id).unwrap().is_none());
    assert_eq!(services.policy_calls.get(), 0);
    assert_eq!(services.nonce_calls.get(), 0);
    assert_eq!(services.signer_calls.get(), 0);
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn unknown_publish_retry_reuses_exact_envelope_without_resigning() {
    let path = directory("retry");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::available();
    services.publisher_available.set(false);
    let gateway = PaymentGateway::new(
        &store, &services, &services, &services, &services, &services,
    );
    assert_eq!(
        gateway.submit(request(50), 100),
        Err(PaymentGatewayError::Service {
            stage: PaymentServiceStage::Publication,
            source: TestError::PublishUnknown,
        })
    );
    assert!(matches!(
        store.get(request(50).request_id).unwrap().unwrap().state(),
        ReservationState::Indeterminate { .. }
    ));

    services.publisher_available.set(true);
    assert!(matches!(
        gateway.submit(request(50), 101).unwrap(),
        PaymentGatewayOutcome::Submitted(reservation)
            if matches!(reservation.state(), ReservationState::Published(_))
    ));
    assert_eq!(services.policy_calls.get(), 1);
    assert_eq!(services.nonce_calls.get(), 1);
    assert_eq!(services.signer_calls.get(), 1);
    assert_eq!(services.verifier_calls.get(), 1);
    let published = services.published.borrow();
    assert_eq!(published.len(), 2);
    assert_eq!(published[0], published[1]);
    drop(published);
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn signer_failure_keeps_nonce_and_retry_does_not_allocate_again() {
    let path = directory("signer");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::available();
    services.signer_available.set(false);
    let gateway = PaymentGateway::new(
        &store, &services, &services, &services, &services, &services,
    );
    assert_eq!(
        gateway.submit(request(50), 100),
        Err(PaymentGatewayError::Service {
            stage: PaymentServiceStage::Signing,
            source: TestError::SignerUnavailable,
        })
    );
    assert!(matches!(
        store.get(request(50).request_id).unwrap().unwrap().state(),
        ReservationState::NonceAssigned(7)
    ));
    services.signer_available.set(true);
    assert!(matches!(
        gateway.submit(request(50), 101).unwrap(),
        PaymentGatewayOutcome::Submitted(_)
    ));
    assert_eq!(services.nonce_calls.get(), 1);
    assert_eq!(services.policy_calls.get(), 1);
    assert_eq!(services.signer_calls.get(), 2);
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn denial_is_durable_before_nonce_and_conflicting_body_fails_closed() {
    let path = directory("denial");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::available();
    services.policy_allowed.set(false);
    let gateway = PaymentGateway::new(
        &store, &services, &services, &services, &services, &services,
    );
    assert_eq!(
        gateway.submit(request(50), 100),
        Err(PaymentGatewayError::Service {
            stage: PaymentServiceStage::Policy,
            source: TestError::Denied,
        })
    );
    assert!(matches!(
        gateway.submit(request(50), 101).unwrap(),
        PaymentGatewayOutcome::Rejected(_)
    ));
    assert_eq!(services.nonce_calls.get(), 0);
    assert_eq!(services.signer_calls.get(), 0);
    assert!(matches!(
        gateway.submit(request(51), 102),
        Err(PaymentGatewayError::Store(_))
    ));
    drop(store);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn signer_cannot_change_any_monetary_field() {
    let path = directory("signer-mutation");
    let store = LmdbPaymentIdempotencyStore::open(
        &path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::available();
    services.signer_mutates_operation.set(true);
    let gateway = PaymentGateway::new(
        &store, &services, &services, &services, &services, &services,
    );
    assert_eq!(
        gateway.submit(request(50), 100),
        Err(PaymentGatewayError::SignedOperationMismatch)
    );
    assert!(matches!(
        store.get(request(50).request_id).unwrap().unwrap().state(),
        ReservationState::NonceAssigned(7)
    ));
    assert_eq!(services.verifier_calls.get(), 0);
    assert!(services.published.borrow().is_empty());
    drop(store);
    fs::remove_dir_all(path).unwrap();
}
