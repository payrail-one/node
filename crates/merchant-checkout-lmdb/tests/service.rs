use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ledger_core::{
    AccountId, AssetId, Authorization, AuthorizedOperation, NetworkId, OperationKind,
    OperationOutcome, OperationReceipt, SignatureBytes, SignedOperation,
};
use merchant_checkout_core::{
    CheckoutDefinition, CheckoutPaymentStatus, CheckoutServiceError, CheckoutState, FeeMode,
    MerchantCheckoutService, MerchantId, MerchantOrderKey,
};
use merchant_checkout_lmdb::{LmdbMerchantCheckoutStore, MerchantCheckoutStoreOptions};
use payment_gateway_core::{
    AccountNonceSource, CanonicalPaymentRequest, PaymentGateway, PaymentGatewayError,
    PaymentOperationSigner, PaymentPolicy, PaymentPublisher, PaymentServiceStage, PublicationAck,
    SignedPaymentVerifier,
};
use payment_idempotency_core::{ReconciliationPageLimit, ReservationState, TenantId};
use payment_idempotency_lmdb::{LmdbPaymentIdempotencyStore, PaymentIdempotencyStoreOptions};
use state_sync_core::{BlockHash, FinalizedCheckpoint, StateRoot, ValidatorSetHash};

const NETWORK: NetworkId = NetworkId::new([21; 32]);
const TENANT: TenantId = TenantId::new([22; 32]);
const MERCHANT_ACCOUNT: AccountId = AccountId::new([23; 32]);
const PAYER: AccountId = AccountId::new([24; 32]);
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn directory(name: &str) -> PathBuf {
    let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "merchant-checkout-service-{name}-{}-{sequence}",
        std::process::id()
    ))
}

fn definition(order: u8) -> CheckoutDefinition {
    CheckoutDefinition {
        network: NETWORK,
        tenant: TENANT,
        merchant: MerchantId::new([25; 32]),
        order_key: MerchantOrderKey::new([order; 32]),
        merchant_account: MERCHANT_ACCOUNT,
        asset: AssetId::new([26; 32]),
        amount: 100,
        maximum_fee: 5,
        fee_mode: FeeMode::CustomerPays,
        expires_at_ms: 1_000,
        valid_until_height: 10_000,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestError {
    SignerUnavailable,
}

struct TestServices {
    signer_available: Cell<bool>,
    signer_calls: Cell<u32>,
    publish_calls: Cell<u32>,
}

impl TestServices {
    fn new() -> Self {
        Self {
            signer_available: Cell::new(true),
            signer_calls: Cell::new(0),
            publish_calls: Cell::new(0),
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
        Ok(0)
    }
}

impl PaymentPolicy for TestServices {
    type Error = TestError;

    fn authorize(&self, _request: &CanonicalPaymentRequest) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl PaymentOperationSigner for TestServices {
    type Error = TestError;

    fn sign(&self, operation: AuthorizedOperation) -> Result<SignedOperation, Self::Error> {
        self.signer_calls.set(self.signer_calls.get() + 1);
        if !self.signer_available.get() {
            return Err(TestError::SignerUnavailable);
        }
        Ok(SignedOperation {
            sender_authorization: Authorization {
                signer: operation.sender(),
                signature: SignatureBytes::new([27; 64]),
            },
            fee_payer_authorization: None,
            operation,
        })
    }
}

impl SignedPaymentVerifier for TestServices {
    type Error = TestError;

    fn verify(&self, _signed: &SignedOperation) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl PaymentPublisher for TestServices {
    type Error = TestError;

    fn publish(&self, _signed: &SignedOperation) -> Result<PublicationAck, Self::Error> {
        self.publish_calls.set(self.publish_calls.get() + 1);
        Ok(PublicationAck::Accepted)
    }
}

fn checkpoint() -> FinalizedCheckpoint {
    FinalizedCheckpoint {
        height: 10,
        block_hash: BlockHash::new([28; 32]),
        state_root: StateRoot::new([29; 32]),
        validator_set_hash: ValidatorSetHash::new([30; 32]),
    }
}

fn finalize_checkout(
    checkouts: &LmdbMerchantCheckoutStore,
    payments: &LmdbPaymentIdempotencyStore,
    definition: CheckoutDefinition,
    outcome: OperationOutcome,
    operation_index: u64,
    now_ms: u64,
) {
    let checkout = checkouts
        .get(TENANT, definition.checkout_id())
        .unwrap()
        .unwrap();
    let CheckoutState::Claimed { claim, .. } = checkout.state() else {
        panic!("paid checkout must be claimed");
    };
    let request_id = claim.payment_request_id(TENANT);
    let reservation = payments.get(request_id).unwrap().unwrap();
    let ReservationState::Published(submission) = reservation.state() else {
        panic!("gateway submission must be published");
    };
    payments
        .finalize(
            request_id,
            reservation.intent().request_digest,
            checkpoint(),
            OperationReceipt {
                operation_id: submission.operation_id,
                account: PAYER,
                idempotency_key: reservation.intent().ledger_idempotency_key,
                nonce: submission.nonce,
                operation_index,
                kind: OperationKind::Transfer,
                outcome,
            },
            now_ms,
        )
        .unwrap();
}

#[test]
fn merchant_status_never_reports_final_before_finalized_receipt() {
    let checkout_path = directory("status-checkouts");
    let payment_path = directory("status-payments");
    let checkouts = LmdbMerchantCheckoutStore::open(
        &checkout_path,
        NETWORK,
        MerchantCheckoutStoreOptions::default(),
    )
    .unwrap();
    let payments = LmdbPaymentIdempotencyStore::open(
        &payment_path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::new();
    let gateway = PaymentGateway::new(
        &payments, &services, &services, &services, &services, &services,
    );
    let merchant = MerchantCheckoutService::new(&checkouts, &payments, &gateway);
    let checkout_definition = definition(1);
    merchant.create(checkout_definition, 100).unwrap();
    merchant
        .pay(TENANT, checkout_definition.checkout_id(), PAYER, 2, 200)
        .unwrap();
    assert_eq!(
        merchant
            .status(TENANT, checkout_definition.checkout_id(), 201)
            .unwrap(),
        CheckoutPaymentStatus::Accepted
    );

    finalize_checkout(
        &checkouts,
        &payments,
        checkout_definition,
        OperationOutcome::Applied,
        0,
        300,
    );
    assert_eq!(
        merchant
            .status(TENANT, checkout_definition.checkout_id(), 301)
            .unwrap(),
        CheckoutPaymentStatus::Finalized
    );
    merchant
        .pay(TENANT, checkout_definition.checkout_id(), PAYER, 2, 1_100)
        .unwrap();
    assert_eq!(services.signer_calls.get(), 1);
    assert_eq!(services.publish_calls.get(), 1);

    let expired_definition = definition(2);
    merchant.create(expired_definition, 400).unwrap();
    merchant
        .pay(TENANT, expired_definition.checkout_id(), PAYER, 2, 450)
        .unwrap();
    finalize_checkout(
        &checkouts,
        &payments,
        expired_definition,
        OperationOutcome::Expired,
        1,
        500,
    );
    assert_eq!(
        merchant
            .status(TENANT, expired_definition.checkout_id(), 501)
            .unwrap(),
        CheckoutPaymentStatus::PaymentExpired
    );
    assert!(matches!(
        merchant
            .pay(TENANT, expired_definition.checkout_id(), PAYER, 2, 502,)
            .unwrap()
            .gateway,
        payment_gateway_core::PaymentGatewayOutcome::Expired(_)
    ));
    assert_eq!(services.signer_calls.get(), 2);
    assert_eq!(services.publish_calls.get(), 2);
    drop(payments);
    drop(checkouts);
    fs::remove_dir_all(checkout_path).unwrap();
    fs::remove_dir_all(payment_path).unwrap();
}

#[test]
fn failed_gateway_dispatch_is_durable_and_retried_after_checkout_restart() {
    let checkout_path = directory("retry-checkouts");
    let payment_path = directory("retry-payments");
    let payments = LmdbPaymentIdempotencyStore::open(
        &payment_path,
        NETWORK,
        PaymentIdempotencyStoreOptions::default(),
    )
    .unwrap();
    let services = TestServices::new();
    services.signer_available.set(false);
    let definition = definition(2);
    {
        let checkouts = LmdbMerchantCheckoutStore::open(
            &checkout_path,
            NETWORK,
            MerchantCheckoutStoreOptions::default(),
        )
        .unwrap();
        let gateway = PaymentGateway::new(
            &payments, &services, &services, &services, &services, &services,
        );
        let merchant = MerchantCheckoutService::new(&checkouts, &payments, &gateway);
        merchant.create(definition, 100).unwrap();
        assert!(matches!(
            merchant.pay(TENANT, definition.checkout_id(), PAYER, 2, 200),
            Err(CheckoutServiceError::Gateway(
                PaymentGatewayError::Service {
                    stage: PaymentServiceStage::Signing,
                    source: TestError::SignerUnavailable,
                }
            ))
        ));
    }

    services.signer_available.set(true);
    let reopened = LmdbMerchantCheckoutStore::open(
        &checkout_path,
        NETWORK,
        MerchantCheckoutStoreOptions::default(),
    )
    .unwrap();
    let gateway = PaymentGateway::new(
        &payments, &services, &services, &services, &services, &services,
    );
    let merchant = MerchantCheckoutService::new(&reopened, &payments, &gateway);
    let batch = merchant
        .dispatch_pending(None, ReconciliationPageLimit::new(10).unwrap(), 300)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert!(batch.items[0].result.is_ok());
    assert!(
        reopened
            .pending_dispatch_after(None, ReconciliationPageLimit::new(10).unwrap())
            .unwrap()
            .checkouts
            .is_empty()
    );
    assert_eq!(
        merchant
            .status(TENANT, definition.checkout_id(), 301)
            .unwrap(),
        CheckoutPaymentStatus::Accepted
    );
    drop(reopened);
    drop(payments);
    fs::remove_dir_all(checkout_path).unwrap();
    fs::remove_dir_all(payment_path).unwrap();
}
