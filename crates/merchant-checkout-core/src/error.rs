use payment_gateway_core::PaymentRequestError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutError {
    ZeroAmount,
    InvalidExpiry,
    InvalidValidity,
    InvalidFeePolicy,
    PayerIsMerchant,
    FeeTooHigh,
    Expired,
    DefinitionConflict,
    ClaimConflict,
    InvalidTransition,
    TimestampRegression,
    CorruptState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckoutServiceError<CheckoutStoreError, PaymentStoreError, GatewayError> {
    Checkout(CheckoutStoreError),
    Payment(PaymentStoreError),
    Gateway(GatewayError),
    InvalidPayment(PaymentRequestError),
    CheckoutNotFound,
    PaymentJournalMissing,
    WrongNetwork,
}
