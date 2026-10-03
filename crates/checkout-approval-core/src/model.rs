use ledger_core::{AccountId, NetworkId};
use merchant_checkout_core::CheckoutId;

use crate::ApprovalCodeError;

pub const APPROVAL_CODE_DIGITS: usize = 6;

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ApprovalCode([u8; APPROVAL_CODE_DIGITS]);

impl ApprovalCode {
    /// Creates a zero-padded six-digit code from its numeric representation.
    ///
    /// # Errors
    ///
    /// Rejects values outside the six-digit code space.
    pub fn from_number(value: u32) -> Result<Self, ApprovalCodeError> {
        if value >= 1_000_000 {
            return Err(ApprovalCodeError::InvalidCode);
        }
        let mut digits = [b'0'; APPROVAL_CODE_DIGITS];
        let mut remaining = value;
        for digit in digits.iter_mut().rev() {
            *digit =
                b'0' + u8::try_from(remaining % 10).map_err(|_| ApprovalCodeError::InvalidCode)?;
            remaining /= 10;
        }
        Ok(Self(digits))
    }

    /// Parses exactly six ASCII digits without accepting whitespace or locale
    /// digits.
    ///
    /// # Errors
    ///
    /// Returns `InvalidCode` when the value is not exactly six ASCII digits.
    pub fn parse(value: &str) -> Result<Self, ApprovalCodeError> {
        let bytes: [u8; APPROVAL_CODE_DIGITS] = value
            .as_bytes()
            .try_into()
            .map_err(|_| ApprovalCodeError::InvalidCode)?;
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(ApprovalCodeError::InvalidCode);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; APPROVAL_CODE_DIGITS] {
        &self.0
    }

    #[must_use]
    pub fn expose(self) -> String {
        self.0.iter().map(|digit| char::from(*digit)).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ApprovalCodeDigest([u8; 32]);

impl ApprovalCodeDigest {
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
pub struct ApprovalCodePolicy {
    pub lifetime_ms: u64,
}

impl ApprovalCodePolicy {
    pub const TWO_MINUTES: Self = Self {
        lifetime_ms: 120_000,
    };

    pub(crate) const fn validate(self) -> Result<(), ApprovalCodeError> {
        if self.lifetime_ms == 0 || self.lifetime_ms > Self::TWO_MINUTES.lifetime_ms {
            Err(ApprovalCodeError::InvalidPolicy)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalCodeClaim {
    pub checkout: CheckoutId,
    pub claimed_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCodeState {
    Ready,
    Claimed(ApprovalCodeClaim),
    Consumed {
        claim: ApprovalCodeClaim,
        consumed_at_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalCodeRecord {
    network: NetworkId,
    digest: ApprovalCodeDigest,
    account: AccountId,
    issued_at_ms: u64,
    expires_at_ms: u64,
    state: ApprovalCodeState,
}

impl ApprovalCodeRecord {
    /// Creates a ready code record with a bounded lifetime.
    ///
    /// # Errors
    ///
    /// Rejects invalid policy and timestamp overflow.
    pub fn issue(
        network: NetworkId,
        digest: ApprovalCodeDigest,
        account: AccountId,
        policy: ApprovalCodePolicy,
        now_ms: u64,
    ) -> Result<Self, ApprovalCodeError> {
        policy.validate()?;
        let expires_at_ms = now_ms
            .checked_add(policy.lifetime_ms)
            .ok_or(ApprovalCodeError::InvalidExpiry)?;
        Ok(Self {
            network,
            digest,
            account,
            issued_at_ms: now_ms,
            expires_at_ms,
            state: ApprovalCodeState::Ready,
        })
    }

    /// Restores a persisted record through all domain invariant checks.
    ///
    /// # Errors
    ///
    /// Rejects invalid timestamps or state transitions.
    pub fn restore(
        network: NetworkId,
        digest: ApprovalCodeDigest,
        account: AccountId,
        issued_at_ms: u64,
        expires_at_ms: u64,
        state: ApprovalCodeState,
    ) -> Result<Self, ApprovalCodeError> {
        let record = Self {
            network,
            digest,
            account,
            issued_at_ms,
            expires_at_ms,
            state,
        };
        record.validate()?;
        Ok(record)
    }

    #[must_use]
    pub const fn network(self) -> NetworkId {
        self.network
    }

    #[must_use]
    pub const fn digest(self) -> ApprovalCodeDigest {
        self.digest
    }

    #[must_use]
    pub const fn account(self) -> AccountId {
        self.account
    }

    #[must_use]
    pub const fn issued_at_ms(self) -> u64 {
        self.issued_at_ms
    }

    #[must_use]
    pub const fn expires_at_ms(self) -> u64 {
        self.expires_at_ms
    }

    #[must_use]
    pub const fn state(self) -> ApprovalCodeState {
        self.state
    }

    #[must_use]
    pub const fn is_expired(self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }

    /// Claims the code for exactly one checkout.
    ///
    /// # Errors
    ///
    /// Rejects expiry, a conflicting checkout or timestamp regression.
    pub fn claim(
        &mut self,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutationOutcome, ApprovalCodeError> {
        self.require_current(now_ms)?;
        match self.state {
            ApprovalCodeState::Ready => {
                self.state = ApprovalCodeState::Claimed(ApprovalCodeClaim {
                    checkout,
                    claimed_at_ms: now_ms,
                });
                Ok(ApprovalCodeMutationOutcome::Applied)
            }
            ApprovalCodeState::Claimed(claim) if claim.checkout == checkout => {
                Ok(ApprovalCodeMutationOutcome::ExistingSame)
            }
            ApprovalCodeState::Claimed(_) | ApprovalCodeState::Consumed { .. } => {
                Err(ApprovalCodeError::AlreadyClaimed)
            }
        }
    }

    /// Marks an exact claimed checkout as consumed after signed payment
    /// finalization.
    ///
    /// # Errors
    ///
    /// Rejects a wrong checkout, regression or an unclaimed record.
    pub fn consume(
        &mut self,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeMutationOutcome, ApprovalCodeError> {
        self.require_time(now_ms)?;
        match self.state {
            ApprovalCodeState::Claimed(claim) if claim.checkout == checkout => {
                self.state = ApprovalCodeState::Consumed {
                    claim,
                    consumed_at_ms: now_ms,
                };
                Ok(ApprovalCodeMutationOutcome::Applied)
            }
            ApprovalCodeState::Consumed {
                claim,
                consumed_at_ms: _,
            } if claim.checkout == checkout => Ok(ApprovalCodeMutationOutcome::ExistingSame),
            ApprovalCodeState::Ready => Err(ApprovalCodeError::InvalidTransition),
            ApprovalCodeState::Claimed(_) | ApprovalCodeState::Consumed { .. } => {
                Err(ApprovalCodeError::AlreadyClaimed)
            }
        }
    }

    fn require_current(self, now_ms: u64) -> Result<(), ApprovalCodeError> {
        self.require_time(now_ms)?;
        if self.is_expired(now_ms) {
            Err(ApprovalCodeError::Expired)
        } else {
            Ok(())
        }
    }

    fn require_time(self, now_ms: u64) -> Result<(), ApprovalCodeError> {
        if now_ms < self.issued_at_ms {
            Err(ApprovalCodeError::TimestampRegression)
        } else {
            Ok(())
        }
    }

    fn validate(self) -> Result<(), ApprovalCodeError> {
        if self.expires_at_ms <= self.issued_at_ms {
            return Err(ApprovalCodeError::CorruptState);
        }
        match self.state {
            ApprovalCodeState::Ready => Ok(()),
            ApprovalCodeState::Claimed(claim) => self.validate_claim(claim),
            ApprovalCodeState::Consumed {
                claim,
                consumed_at_ms,
            } => {
                self.validate_claim(claim)?;
                if consumed_at_ms < claim.claimed_at_ms {
                    Err(ApprovalCodeError::CorruptState)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn validate_claim(self, claim: ApprovalCodeClaim) -> Result<(), ApprovalCodeError> {
        if claim.claimed_at_ms < self.issued_at_ms || claim.claimed_at_ms >= self.expires_at_ms {
            Err(ApprovalCodeError::CorruptState)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalCodeMutationOutcome {
    Applied,
    ExistingSame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalCodeMutation {
    pub outcome: ApprovalCodeMutationOutcome,
    pub record: ApprovalCodeRecord,
}

// Deliberately no `Debug`: the plaintext code must not enter structured logs.
#[derive(Clone, Eq, PartialEq)]
pub struct IssuedApprovalCode {
    pub code: String,
    pub account: AccountId,
    pub expires_at_ms: u64,
}
