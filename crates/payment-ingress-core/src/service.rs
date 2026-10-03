use payment_gateway_core::{CanonicalPaymentRequest, PaymentGatewayOutcome};

use crate::{
    PaymentIngressError, PaymentIngressIdentity, PaymentIngressJournal, PaymentIngressRateLimiter,
    PaymentIngressResult, PaymentSubmissionGateway, RateLimitDecision,
};

#[derive(Debug)]
pub struct PaymentIngressService<'a, Journal, Limiter, Gateway> {
    journal: &'a Journal,
    limiter: &'a mut Limiter,
    gateway: &'a Gateway,
}

impl<'a, Journal, Limiter, Gateway> PaymentIngressService<'a, Journal, Limiter, Gateway> {
    #[must_use]
    pub const fn new(journal: &'a Journal, limiter: &'a mut Limiter, gateway: &'a Gateway) -> Self {
        Self {
            journal,
            limiter,
            gateway,
        }
    }
}

impl<Journal, Limiter, Gateway> PaymentIngressService<'_, Journal, Limiter, Gateway>
where
    Journal: PaymentIngressJournal,
    Limiter: PaymentIngressRateLimiter,
    Gateway: PaymentSubmissionGateway,
{
    /// Classifies a canonical request against the durable journal, applies
    /// bounded time-window quotas and only then invokes the payment gateway.
    ///
    /// Every valid authorized request consumes API-principal quota before a
    /// journal lookup. New intents then consume tenant and account quota
    /// atomically. Exact retries and conflicting reuse of a request ID do not
    /// consume monetary-identity quotas again.
    ///
    /// The complete identity must be derived from authenticated and authorized
    /// transport credentials; it must never be accepted from untrusted request
    /// fields. Its tenant and account must match the canonical request.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, journal, limiter, rate-limit or gateway
    /// error. A limited or conflicting request never reaches the gateway.
    pub fn submit(
        &mut self,
        identity: PaymentIngressIdentity,
        request: CanonicalPaymentRequest,
        now_ms: u64,
    ) -> PaymentIngressResult<PaymentGatewayOutcome, Journal::Error, Limiter::Error, Gateway::Error>
    {
        let intent = request
            .intent()
            .map_err(PaymentIngressError::InvalidRequest)?;
        if request.network != self.journal.network() {
            return Err(PaymentIngressError::WrongNetwork);
        }
        if identity.tenant != request.request_id.tenant
            || identity.account != request.request_id.account
        {
            return Err(PaymentIngressError::IdentityMismatch);
        }
        require_admitted(
            self.limiter
                .admit_principal(identity.principal, now_ms)
                .map_err(PaymentIngressError::Limiter)?,
        )?;
        let existing = self
            .journal
            .get(request.request_id)
            .map_err(PaymentIngressError::Journal)?;
        if existing
            .as_ref()
            .is_some_and(|stored| stored.intent().request_digest != intent.request_digest)
        {
            return Err(PaymentIngressError::RequestConflict);
        }
        if existing.is_none() {
            require_admitted(
                self.limiter
                    .admit_new_intent(identity.tenant, identity.account, now_ms)
                    .map_err(PaymentIngressError::Limiter)?,
            )?;
        }
        self.gateway
            .submit(request, now_ms)
            .map_err(PaymentIngressError::Gateway)
    }
}

fn require_admitted<JournalError, LimiterError, GatewayError>(
    decision: RateLimitDecision,
) -> PaymentIngressResult<(), JournalError, LimiterError, GatewayError> {
    match decision {
        RateLimitDecision::Admitted => Ok(()),
        RateLimitDecision::Limited { scope, retry_at_ms } => {
            Err(PaymentIngressError::RateLimited { scope, retry_at_ms })
        }
    }
}
