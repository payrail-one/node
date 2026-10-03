#![forbid(unsafe_code)]

mod error;
mod model;
mod port;
mod service;

pub use error::{CheckoutError, CheckoutServiceError};
pub use model::{
    Checkout, CheckoutClaim, CheckoutDefinition, CheckoutDispatchState, CheckoutId,
    CheckoutMutation, CheckoutMutationOutcome, CheckoutPaymentStatus, CheckoutState, FeeMode,
    MerchantId, MerchantOrderKey, PendingCheckoutPage,
};
pub use port::{CheckoutPaymentGateway, MerchantCheckoutStore};
pub use service::{
    CheckoutDispatchBatch, CheckoutDispatchBatchResult, CheckoutDispatchItem,
    CheckoutDispatchResult, CheckoutPaymentOutcome, CheckoutServiceResult, MerchantCheckoutService,
};
