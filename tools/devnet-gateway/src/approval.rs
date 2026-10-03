use std::path::Path;

use checkout_approval_auth_hmac::HmacApprovalCodeAuthenticator;
use checkout_approval_core::{
    ApprovalCodeAuthenticator, ApprovalCodeDigest, ApprovalCodeGenerator, ApprovalCodePolicy,
    ApprovalCodeRecord, ApprovalCodeService, ApprovalCodeServiceError,
};
use checkout_approval_lmdb::{
    ApprovalCodeStoreError, ApprovalCodeStoreOptions, LmdbApprovalCodeStore,
};
use checkout_approval_random::SystemApprovalCodeGenerator;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use ledger_core::AccountId;
use merchant_checkout_core::CheckoutId;
use payment_idempotency_core::TenantId;
use payment_ingress_core::{
    ApiPrincipalId, BucketPolicy, PaymentRateLimitConfig, RateLimitDecision, TrackingCapacities,
};
use payment_rate_limit_lmdb::{LmdbPaymentRateLimiter, LmdbRateLimitOptions};
use sha2_legacy::Sha256;

use crate::{
    DevnetError,
    codec::{decode_bounded_hex, encode_hex},
    model::IssuedApprovalCodeView,
    service::NETWORK,
};

const ISSUE_DOMAIN: &[u8] = b"payrail.approval.issue.v1\0";
const SESSION_DOMAIN: &[u8] = b"payrail.approval.session.v1\0";
const MERCHANT_DOMAIN: &[u8] = b"payrail.approval.merchant.v1\0";
const MAX_CLOCK_SKEW_MS: u64 = 30_000;
const SESSION_BYTES: usize = 72;
const TOKEN_ENV: &str = "DEVNET_APPROVAL_MERCHANT_TOKEN";
const KEY_ENV: &str = "DEVNET_APPROVAL_HMAC_KEY_HEX";

type HmacSha256 = Hmac<Sha256>;

pub(crate) struct ApprovalRail {
    authenticator: HmacApprovalCodeAuthenticator,
    store: LmdbApprovalCodeStore,
    generator: SystemApprovalCodeGenerator,
    limiter: LmdbPaymentRateLimiter,
    session_mac: HmacSha256,
    merchant_mac: HmacSha256,
    merchant_digest: [u8; 32],
}

impl ApprovalRail {
    pub(crate) fn from_environment(root: &Path) -> Result<Option<Self>, DevnetError> {
        let (Ok(encoded_key), Ok(merchant_token)) =
            (std::env::var(KEY_ENV), std::env::var(TOKEN_ENV))
        else {
            return Ok(None);
        };
        let key = decode_bounded_hex(&encoded_key, 64)?;
        if key.len() < 32 || merchant_token.len() < 32 || merchant_token.len() > 256 {
            return Err(DevnetError::ApprovalUnavailable);
        }
        Self::open(root, &key, merchant_token.as_bytes()).map(Some)
    }

    pub(crate) fn open(
        root: &Path,
        key: &[u8],
        merchant_token: &[u8],
    ) -> Result<Self, DevnetError> {
        let authenticator = HmacApprovalCodeAuthenticator::new(key)
            .map_err(|_| DevnetError::ApprovalUnavailable)?;
        let store = LmdbApprovalCodeStore::open(
            root.join("approval-codes"),
            NETWORK,
            ApprovalCodeStoreOptions::default(),
        )
        .map_err(|_| DevnetError::StateUnavailable)?;
        let limiter = LmdbPaymentRateLimiter::open(
            root.join("approval-rate-limits"),
            NETWORK,
            rate_limit_config()?,
            LmdbRateLimitOptions::default(),
        )
        .map_err(|_| DevnetError::StateUnavailable)?;
        let session_mac =
            HmacSha256::new_from_slice(key).map_err(|_| DevnetError::ApprovalUnavailable)?;
        let merchant_mac = session_mac.clone();
        let merchant_digest = merchant_digest(&merchant_mac, merchant_token);
        Ok(Self {
            authenticator,
            store,
            generator: SystemApprovalCodeGenerator,
            limiter,
            session_mac,
            merchant_mac,
            merchant_digest,
        })
    }

    pub(crate) fn issue(
        &self,
        account: AccountId,
        device: [u8; 32],
        issued_at_ms: u64,
        nonce: [u8; 32],
        signature: [u8; 64],
        now_ms: u64,
    ) -> Result<IssuedApprovalCodeView, DevnetError> {
        verify_issue(account, device, issued_at_ms, nonce, signature, now_ms)?;
        require_admitted(
            self.limiter
                .admit_principal(ApiPrincipalId::new(device), now_ms),
        )?;
        require_admitted(self.limiter.admit_new_intent(
            TenantId::new([0x49; 32]),
            account,
            now_ms,
        ))?;
        let service = ApprovalCodeService::new(
            &self.authenticator,
            &self.store,
            ApprovalCodePolicy::TWO_MINUTES,
        );
        for _ in 0..8 {
            let code = self
                .generator
                .generate()
                .map_err(|_| DevnetError::InternalInvariant)?;
            let digest = self.authenticator.digest(NETWORK, code);
            match service.issue(account, code, now_ms) {
                Ok(issued) => {
                    return Ok(IssuedApprovalCodeView {
                        code: issued.code,
                        session_token: self.session_token(digest, issued.expires_at_ms),
                        expires_at_ms: issued.expires_at_ms.to_string(),
                    });
                }
                Err(
                    ApprovalCodeServiceError::CodeCollision
                    | ApprovalCodeServiceError::Store(ApprovalCodeStoreError::CodeCollision),
                ) => {}
                Err(error) => return Err(map_service_error(&error)),
            }
        }
        Err(DevnetError::ApprovalUnavailable)
    }

    pub(crate) fn authorize_merchant(
        &self,
        authorization: Option<&str>,
    ) -> Result<(), DevnetError> {
        let token = authorization
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or(DevnetError::ApprovalUnauthorized)?;
        if token.len() < 32 || token.len() > 256 {
            return Err(DevnetError::ApprovalUnauthorized);
        }
        let mut mac = self.merchant_mac.clone();
        mac.update(MERCHANT_DOMAIN);
        mac.update(token.as_bytes());
        mac.verify_slice(&self.merchant_digest)
            .map_err(|_| DevnetError::ApprovalUnauthorized)
    }

    pub(crate) fn claim(
        &self,
        code: &str,
        checkout: CheckoutId,
        now_ms: u64,
    ) -> Result<ApprovalCodeRecord, DevnetError> {
        require_admitted(
            self.limiter
                .admit_principal(ApiPrincipalId::new(self.merchant_digest), now_ms),
        )?;
        ApprovalCodeService::new(
            &self.authenticator,
            &self.store,
            ApprovalCodePolicy::TWO_MINUTES,
        )
        .claim(code, checkout, now_ms)
        .map(|mutation| mutation.record)
        .map_err(|error| map_service_error(&error))
    }

    pub(crate) fn record_for_session(
        &self,
        token: &str,
        now_ms: u64,
    ) -> Result<ApprovalCodeRecord, DevnetError> {
        let digest = self.verify_session(token, now_ms)?;
        ApprovalCodeService::new(
            &self.authenticator,
            &self.store,
            ApprovalCodePolicy::TWO_MINUTES,
        )
        .by_digest(digest)
        .map_err(|error| map_service_error(&error))?
        .ok_or(DevnetError::ApprovalNotFound)
    }

    pub(crate) fn record_for_checkout(
        &self,
        checkout: CheckoutId,
    ) -> Result<Option<ApprovalCodeRecord>, DevnetError> {
        ApprovalCodeService::new(
            &self.authenticator,
            &self.store,
            ApprovalCodePolicy::TWO_MINUTES,
        )
        .by_checkout(checkout)
        .map_err(|error| map_service_error(&error))
    }

    pub(crate) fn consume(
        &self,
        checkout: CheckoutId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<(), DevnetError> {
        ApprovalCodeService::new(
            &self.authenticator,
            &self.store,
            ApprovalCodePolicy::TWO_MINUTES,
        )
        .consume(checkout, account, now_ms)
        .map(|_| ())
        .map_err(|error| map_service_error(&error))
    }

    fn session_token(&self, digest: ApprovalCodeDigest, expires_at_ms: u64) -> String {
        let mut payload = [0_u8; 40];
        payload[..32].copy_from_slice(digest.as_bytes());
        payload[32..].copy_from_slice(&expires_at_ms.to_be_bytes());
        let mut mac = self.session_mac.clone();
        mac.update(SESSION_DOMAIN);
        mac.update(&payload);
        let tag: [u8; 32] = mac.finalize().into_bytes().into();
        let mut token = [0_u8; SESSION_BYTES];
        token[..40].copy_from_slice(&payload);
        token[40..].copy_from_slice(&tag);
        encode_hex(&token)
    }

    fn verify_session(&self, token: &str, now_ms: u64) -> Result<ApprovalCodeDigest, DevnetError> {
        let token: [u8; SESSION_BYTES] = decode_bounded_hex(token, SESSION_BYTES)?
            .try_into()
            .map_err(|_| DevnetError::ApprovalInvalid)?;
        let expires_at_ms = u64::from_be_bytes(
            token[32..40]
                .try_into()
                .map_err(|_| DevnetError::ApprovalInvalid)?,
        );
        if now_ms >= expires_at_ms {
            return Err(DevnetError::ApprovalInvalid);
        }
        let mut mac = self.session_mac.clone();
        mac.update(SESSION_DOMAIN);
        mac.update(&token[..40]);
        mac.verify_slice(&token[40..])
            .map_err(|_| DevnetError::ApprovalInvalid)?;
        Ok(ApprovalCodeDigest::new(
            token[..32]
                .try_into()
                .map_err(|_| DevnetError::ApprovalInvalid)?,
        ))
    }
}

fn verify_issue(
    account: AccountId,
    device: [u8; 32],
    issued_at_ms: u64,
    nonce: [u8; 32],
    signature: [u8; 64],
    now_ms: u64,
) -> Result<(), DevnetError> {
    if issued_at_ms.abs_diff(now_ms) > MAX_CLOCK_SKEW_MS {
        return Err(DevnetError::ApprovalInvalid);
    }
    let key =
        VerifyingKey::from_bytes(account.as_bytes()).map_err(|_| DevnetError::ApprovalInvalid)?;
    let mut message = Vec::with_capacity(ISSUE_DOMAIN.len() + 104);
    message.extend_from_slice(ISSUE_DOMAIN);
    message.extend_from_slice(NETWORK.as_bytes());
    message.extend_from_slice(account.as_bytes());
    message.extend_from_slice(&device);
    message.extend_from_slice(&issued_at_ms.to_be_bytes());
    message.extend_from_slice(&nonce);
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| DevnetError::ApprovalUnauthorized)
}

fn merchant_digest(template: &HmacSha256, token: &[u8]) -> [u8; 32] {
    let mut mac = template.clone();
    mac.update(MERCHANT_DOMAIN);
    mac.update(token);
    mac.finalize().into_bytes().into()
}

fn rate_limit_config() -> Result<PaymentRateLimitConfig, DevnetError> {
    PaymentRateLimitConfig::new(
        BucketPolicy::new(20, 6_000).map_err(|_| DevnetError::InternalInvariant)?,
        BucketPolicy::new(50, 2_400).map_err(|_| DevnetError::InternalInvariant)?,
        BucketPolicy::new(3, 40_000).map_err(|_| DevnetError::InternalInvariant)?,
        TrackingCapacities {
            principals: 1_024,
            tenants: 16,
            accounts: 100_000,
        },
        120_000,
    )
    .map_err(|_| DevnetError::InternalInvariant)
}

fn require_admitted(
    decision: Result<RateLimitDecision, payment_rate_limit_lmdb::LmdbRateLimitError>,
) -> Result<(), DevnetError> {
    match decision.map_err(|_| DevnetError::StateUnavailable)? {
        RateLimitDecision::Admitted => Ok(()),
        RateLimitDecision::Limited { .. } => Err(DevnetError::ApprovalRateLimited),
    }
}

fn map_service_error(error: &ApprovalCodeServiceError<ApprovalCodeStoreError>) -> DevnetError {
    match error {
        ApprovalCodeServiceError::Domain(error)
        | ApprovalCodeServiceError::Store(ApprovalCodeStoreError::Domain(error)) => {
            map_domain_error(*error)
        }
        ApprovalCodeServiceError::CodeNotFound
        | ApprovalCodeServiceError::Store(ApprovalCodeStoreError::CodeNotFound) => {
            DevnetError::ApprovalInvalid
        }
        ApprovalCodeServiceError::Store(_) | ApprovalCodeServiceError::CodeCollision => {
            DevnetError::StateUnavailable
        }
    }
}

fn map_domain_error(error: checkout_approval_core::ApprovalCodeError) -> DevnetError {
    match error {
        checkout_approval_core::ApprovalCodeError::AlreadyClaimed
        | checkout_approval_core::ApprovalCodeError::AccountHasClaimedCode
        | checkout_approval_core::ApprovalCodeError::CheckoutAlreadyLinked => {
            DevnetError::ApprovalConflict
        }
        _ => DevnetError::ApprovalInvalid,
    }
}
