use ledger_core::{AccountId, AssetId, Balance, NetworkId};
use payment_idempotency_core::{ClientRequestKey, PaymentRequestId, TenantId};
use sha2::{Digest, Sha256};

use crate::CheckoutError;

const CHECKOUT_ID_DOMAIN: &[u8] = b"merchant.checkout.id\0";
const PAYMENT_KEY_DOMAIN: &[u8] = b"merchant.checkout.payment-key\0";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MerchantId([u8; 32]);

impl MerchantId {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MerchantOrderKey([u8; 32]);

impl MerchantOrderKey {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CheckoutId([u8; 32]);

impl CheckoutId {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeeMode {
    CustomerPays,
    MerchantSponsored,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckoutDefinition {
    pub network: NetworkId,
    pub tenant: TenantId,
    pub merchant: MerchantId,
    pub order_key: MerchantOrderKey,
    pub merchant_account: AccountId,
    pub asset: AssetId,
    pub amount: Balance,
    pub maximum_fee: Balance,
    pub fee_mode: FeeMode,
    pub expires_at_ms: u64,
    pub valid_until_height: u64,
}

impl CheckoutDefinition {
    #[must_use]
    pub fn checkout_id(self) -> CheckoutId {
        let mut digest = Sha256::new();
        digest.update(CHECKOUT_ID_DOMAIN);
        digest.update(self.network.as_bytes());
        digest.update(self.tenant.as_bytes());
        digest.update(self.merchant.as_bytes());
        digest.update(self.order_key.as_bytes());
        CheckoutId::new(digest.finalize().into())
    }

    pub(crate) const fn validate_static(self) -> Result<(), CheckoutError> {
        if self.amount == 0 {
            return Err(CheckoutError::ZeroAmount);
        }
        if matches!(self.fee_mode, FeeMode::MerchantSponsored) && self.maximum_fee == 0 {
            return Err(CheckoutError::InvalidFeePolicy);
        }
        if self.valid_until_height == 0 {
            return Err(CheckoutError::InvalidValidity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckoutClaim {
    pub payer: AccountId,
    pub client_key: ClientRequestKey,
    pub fee: Balance,
    pub claimed_at_ms: u64,
}

impl CheckoutClaim {
    #[must_use]
    pub const fn payment_request_id(self, tenant: TenantId) -> PaymentRequestId {
        PaymentRequestId {
            tenant,
            account: self.payer,
            client_key: self.client_key,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutDispatchState {
    Pending,
    GatewayRecorded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutState {
    Open,
    Claimed {
        claim: CheckoutClaim,
        dispatch: CheckoutDispatchState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Checkout {
    definition: CheckoutDefinition,
    created_at_ms: u64,
    updated_at_ms: u64,
    state: CheckoutState,
}

impl Checkout {
    /// Creates a checkout whose expiry is strictly in the future.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid monetary policy or expiry.
    pub fn new(definition: CheckoutDefinition, now_ms: u64) -> Result<Self, CheckoutError> {
        definition.validate_static()?;
        if definition.expires_at_ms <= now_ms {
            return Err(CheckoutError::InvalidExpiry);
        }
        Ok(Self {
            definition,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            state: CheckoutState::Open,
        })
    }

    /// Restores a persisted checkout through domain invariant validation.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid timestamps, definition or state.
    pub fn restore(
        definition: CheckoutDefinition,
        created_at_ms: u64,
        updated_at_ms: u64,
        state: CheckoutState,
    ) -> Result<Self, CheckoutError> {
        definition.validate_static()?;
        if definition.expires_at_ms <= created_at_ms || updated_at_ms < created_at_ms {
            return Err(CheckoutError::CorruptState);
        }
        let checkout = Self {
            definition,
            created_at_ms,
            updated_at_ms,
            state,
        };
        checkout.validate_state()?;
        Ok(checkout)
    }

    #[must_use]
    pub const fn definition(self) -> CheckoutDefinition {
        self.definition
    }

    #[must_use]
    pub fn id(self) -> CheckoutId {
        self.definition.checkout_id()
    }

    #[must_use]
    pub const fn created_at_ms(self) -> u64 {
        self.created_at_ms
    }

    #[must_use]
    pub const fn updated_at_ms(self) -> u64 {
        self.updated_at_ms
    }

    #[must_use]
    pub const fn state(self) -> CheckoutState {
        self.state
    }

    /// Claims an open checkout for exactly one payer and fee selection.
    /// Exact retries remain valid after expiry; a different claim never does.
    ///
    /// # Errors
    ///
    /// Returns an error for expiry, fee policy, self-payment, conflicting claim
    /// or regressing time.
    pub fn claim(
        &mut self,
        payer: AccountId,
        fee: Balance,
        now_ms: u64,
    ) -> Result<CheckoutMutationOutcome, CheckoutError> {
        self.require_time(now_ms)?;
        match self.state {
            CheckoutState::Claimed {
                claim: existing, ..
            } if existing.payer == payer && existing.fee == fee => {
                Ok(CheckoutMutationOutcome::ExistingSame)
            }
            CheckoutState::Claimed { .. } => Err(CheckoutError::ClaimConflict),
            CheckoutState::Open => {
                let claim = CheckoutClaim {
                    payer,
                    client_key: payment_client_key(self.id()),
                    fee,
                    claimed_at_ms: now_ms,
                };
                self.validate_new_claim(claim, now_ms)?;
                self.state = CheckoutState::Claimed {
                    claim,
                    dispatch: CheckoutDispatchState::Pending,
                };
                self.updated_at_ms = now_ms;
                Ok(CheckoutMutationOutcome::Applied)
            }
        }
    }

    /// Marks a claimed payment as durably represented in the gateway journal.
    ///
    /// # Errors
    ///
    /// Returns an error for an unclaimed checkout or regressing time.
    pub fn mark_gateway_recorded(
        &mut self,
        now_ms: u64,
    ) -> Result<CheckoutMutationOutcome, CheckoutError> {
        self.require_time(now_ms)?;
        match self.state {
            CheckoutState::Claimed {
                claim,
                dispatch: CheckoutDispatchState::Pending,
            } => {
                self.state = CheckoutState::Claimed {
                    claim,
                    dispatch: CheckoutDispatchState::GatewayRecorded,
                };
                self.updated_at_ms = now_ms;
                Ok(CheckoutMutationOutcome::Applied)
            }
            CheckoutState::Claimed {
                dispatch: CheckoutDispatchState::GatewayRecorded,
                ..
            } => Ok(CheckoutMutationOutcome::ExistingSame),
            CheckoutState::Open => Err(CheckoutError::InvalidTransition),
        }
    }

    fn require_time(self, now_ms: u64) -> Result<(), CheckoutError> {
        if now_ms < self.updated_at_ms {
            Err(CheckoutError::TimestampRegression)
        } else {
            Ok(())
        }
    }

    fn validate_new_claim(self, claim: CheckoutClaim, now_ms: u64) -> Result<(), CheckoutError> {
        if now_ms >= self.definition.expires_at_ms {
            return Err(CheckoutError::Expired);
        }
        if claim.payer == self.definition.merchant_account {
            return Err(CheckoutError::PayerIsMerchant);
        }
        if claim.fee > self.definition.maximum_fee {
            return Err(CheckoutError::FeeTooHigh);
        }
        if matches!(self.definition.fee_mode, FeeMode::MerchantSponsored) && claim.fee == 0 {
            return Err(CheckoutError::InvalidFeePolicy);
        }
        Ok(())
    }

    fn validate_state(self) -> Result<(), CheckoutError> {
        match self.state {
            CheckoutState::Open => Ok(()),
            CheckoutState::Claimed { claim, .. } => {
                if claim.claimed_at_ms < self.created_at_ms
                    || claim.claimed_at_ms >= self.definition.expires_at_ms
                    || claim.client_key != payment_client_key(self.id())
                {
                    return Err(CheckoutError::CorruptState);
                }
                self.validate_new_claim(claim, claim.claimed_at_ms)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutMutationOutcome {
    Applied,
    ExistingSame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckoutMutation {
    pub outcome: CheckoutMutationOutcome,
    pub checkout: Checkout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingCheckoutPage {
    pub checkouts: Vec<Checkout>,
    pub next_cursor: Option<CheckoutId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckoutPaymentStatus {
    Open,
    Expired,
    PaymentExpired,
    PaymentStarting,
    Processing,
    OutcomeUnknown,
    Accepted,
    Finalized,
    Rejected,
}

fn payment_client_key(checkout_id: CheckoutId) -> ClientRequestKey {
    let mut digest = Sha256::new();
    digest.update(PAYMENT_KEY_DOMAIN);
    digest.update(checkout_id.as_bytes());
    ClientRequestKey::new(digest.finalize().into())
}
