use ledger_core::{
    AccountId, AssetId, AuthorizedOperation, Balance, IdempotencyKey, NetworkId, Nonce, Transfer,
};
use payment_idempotency_core::{PaymentIntent, PaymentRequestId, RequestDigest};
use sha2::{Digest, Sha256};

const REQUEST_DIGEST_DOMAIN: &[u8] = b"payment.gateway.request\0";
const LEDGER_KEY_DOMAIN: &[u8] = b"payment.gateway.ledger-key\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentRequestError {
    ZeroAmount,
    SelfTransfer,
    InvalidFeeSponsor,
    InvalidValidity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalPaymentRequest {
    pub network: NetworkId,
    pub request_id: PaymentRequestId,
    pub asset: AssetId,
    pub recipient: AccountId,
    pub amount: Balance,
    pub fee: Balance,
    pub fee_payer: Option<AccountId>,
    pub valid_until_height: u64,
}

impl CanonicalPaymentRequest {
    /// Validates the API-level invariants that must fail before nonce allocation.
    ///
    /// # Errors
    ///
    /// Returns an error for zero-value or self payments and invalid fee
    /// sponsorship.
    pub fn validate(self) -> Result<Self, PaymentRequestError> {
        if self.amount == 0 {
            return Err(PaymentRequestError::ZeroAmount);
        }
        if self.request_id.account == self.recipient {
            return Err(PaymentRequestError::SelfTransfer);
        }
        if self
            .fee_payer
            .is_some_and(|payer| payer == self.request_id.account || self.fee == 0)
        {
            return Err(PaymentRequestError::InvalidFeeSponsor);
        }
        if self.valid_until_height == 0 {
            return Err(PaymentRequestError::InvalidValidity);
        }
        Ok(self)
    }

    /// Produces the immutable reservation intent and deterministic ledger
    /// correlation key.
    ///
    /// # Errors
    ///
    /// Returns an error when API-level payment invariants are invalid.
    pub fn intent(self) -> Result<PaymentIntent, PaymentRequestError> {
        let request = self.validate()?;
        let request_digest = request.request_digest();
        let mut ledger_key = Sha256::new();
        ledger_key.update(LEDGER_KEY_DOMAIN);
        ledger_key.update(request.network.as_bytes());
        ledger_key.update(request.request_id.tenant.as_bytes());
        ledger_key.update(request.request_id.account.as_bytes());
        ledger_key.update(request.request_id.client_key.as_bytes());
        ledger_key.update(request_digest.as_bytes());
        Ok(PaymentIntent {
            network: request.network,
            request_id: request.request_id,
            request_digest,
            ledger_idempotency_key: IdempotencyKey::new(ledger_key.finalize().into()),
        })
    }

    #[must_use]
    pub fn request_digest(self) -> RequestDigest {
        let mut digest = Sha256::new();
        digest.update(REQUEST_DIGEST_DOMAIN);
        digest.update(self.network.as_bytes());
        digest.update(self.request_id.tenant.as_bytes());
        digest.update(self.request_id.account.as_bytes());
        digest.update(self.request_id.client_key.as_bytes());
        digest.update(self.asset.as_bytes());
        digest.update(self.recipient.as_bytes());
        digest.update(self.amount.to_be_bytes());
        digest.update(self.fee.to_be_bytes());
        match self.fee_payer {
            None => digest.update([0]),
            Some(payer) => {
                digest.update([1]);
                digest.update(payer.as_bytes());
            }
        }
        digest.update(self.valid_until_height.to_be_bytes());
        RequestDigest::new(digest.finalize().into())
    }

    /// Creates the exact ledger operation after durable nonce assignment.
    ///
    /// # Errors
    ///
    /// Returns an error when API-level payment invariants are invalid.
    pub fn operation(self, nonce: Nonce) -> Result<AuthorizedOperation, PaymentRequestError> {
        let request = self.validate()?;
        let intent = request.intent()?;
        let transfer = Transfer {
            network: request.network,
            idempotency_key: intent.ledger_idempotency_key,
            asset: request.asset,
            from: request.request_id.account,
            to: request.recipient,
            amount: request.amount,
            fee: request.fee,
            nonce,
            valid_until_height: request.valid_until_height,
        };
        Ok(match request.fee_payer {
            None => AuthorizedOperation::Transfer(transfer),
            Some(fee_payer) => AuthorizedOperation::SponsoredTransfer {
                transfer,
                fee_payer,
            },
        })
    }
}
