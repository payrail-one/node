use payment_ingress_core::{BucketPolicy, PaymentRateLimitConfig, RateLimitScope};

use crate::LmdbRateLimitError;

const CONFIG_MAGIC: [u8; 8] = *b"PRLIM001";
const CONFIG_LENGTH: usize = 76;
const BUCKET_LENGTH: usize = 20;
pub(crate) const EXPIRATION_KEY_LENGTH: usize = 41;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BucketState {
    pub tokens: u32,
    pub last_refill_ms: u64,
    pub expires_at_ms: u64,
}

pub(crate) fn encode_config(
    config: PaymentRateLimitConfig,
) -> Result<[u8; CONFIG_LENGTH], LmdbRateLimitError> {
    let mut encoded = [0_u8; CONFIG_LENGTH];
    encoded[..8].copy_from_slice(&CONFIG_MAGIC);
    let mut offset = 8;
    for policy in [config.principal(), config.tenant(), config.account()] {
        encoded[offset..offset + 4].copy_from_slice(&policy.capacity().to_be_bytes());
        offset += 4;
        encoded[offset..offset + 8].copy_from_slice(&policy.refill_interval_ms().to_be_bytes());
        offset += 8;
    }
    for capacity in [
        config.tracking().principals,
        config.tracking().tenants,
        config.tracking().accounts,
    ] {
        let capacity = u64::try_from(capacity).map_err(|_| LmdbRateLimitError::IntegerOverflow)?;
        encoded[offset..offset + 8].copy_from_slice(&capacity.to_be_bytes());
        offset += 8;
    }
    encoded[offset..offset + 8].copy_from_slice(&config.idle_ttl_ms().to_be_bytes());
    Ok(encoded)
}

pub(crate) fn encode_bucket(state: BucketState) -> [u8; BUCKET_LENGTH] {
    let mut encoded = [0_u8; BUCKET_LENGTH];
    encoded[..4].copy_from_slice(&state.tokens.to_be_bytes());
    encoded[4..12].copy_from_slice(&state.last_refill_ms.to_be_bytes());
    encoded[12..].copy_from_slice(&state.expires_at_ms.to_be_bytes());
    encoded
}

pub(crate) fn decode_bucket(input: &[u8]) -> Result<BucketState, LmdbRateLimitError> {
    if input.len() != BUCKET_LENGTH {
        return Err(LmdbRateLimitError::CorruptRecord);
    }
    Ok(BucketState {
        tokens: u32::from_be_bytes(array(&input[..4])?),
        last_refill_ms: u64::from_be_bytes(array(&input[4..12])?),
        expires_at_ms: u64::from_be_bytes(array(&input[12..])?),
    })
}

pub(crate) fn encode_time(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

pub(crate) fn decode_time(input: &[u8]) -> Result<u64, LmdbRateLimitError> {
    Ok(u64::from_be_bytes(array(input)?))
}

pub(crate) fn expiration_key(
    scope: RateLimitScope,
    expires_at_ms: u64,
    identity: &[u8; 32],
) -> [u8; EXPIRATION_KEY_LENGTH] {
    let mut key = [0_u8; EXPIRATION_KEY_LENGTH];
    key[0] = scope_discriminant(scope);
    key[1..9].copy_from_slice(&expires_at_ms.to_be_bytes());
    key[9..].copy_from_slice(identity);
    key
}

pub(crate) fn expiration_bounds(
    scope: RateLimitScope,
    now_ms: u64,
) -> ([u8; EXPIRATION_KEY_LENGTH], [u8; EXPIRATION_KEY_LENGTH]) {
    let mut minimum = [0_u8; EXPIRATION_KEY_LENGTH];
    minimum[0] = scope_discriminant(scope);
    let mut maximum = [u8::MAX; EXPIRATION_KEY_LENGTH];
    maximum[0] = scope_discriminant(scope);
    maximum[1..9].copy_from_slice(&now_ms.to_be_bytes());
    (minimum, maximum)
}

pub(crate) fn decode_expiration_key(
    input: &[u8],
) -> Result<(RateLimitScope, u64, [u8; 32]), LmdbRateLimitError> {
    if input.len() != EXPIRATION_KEY_LENGTH {
        return Err(LmdbRateLimitError::CorruptRecord);
    }
    Ok((
        decode_scope(input[0])?,
        u64::from_be_bytes(array(&input[1..9])?),
        array(&input[9..])?,
    ))
}

pub(crate) const fn policy_for(
    config: PaymentRateLimitConfig,
    scope: RateLimitScope,
) -> BucketPolicy {
    match scope {
        RateLimitScope::ApiPrincipal => config.principal(),
        RateLimitScope::Tenant => config.tenant(),
        RateLimitScope::Account => config.account(),
    }
}

pub(crate) const fn tracking_capacity(
    config: PaymentRateLimitConfig,
    scope: RateLimitScope,
) -> usize {
    match scope {
        RateLimitScope::ApiPrincipal => config.tracking().principals,
        RateLimitScope::Tenant => config.tracking().tenants,
        RateLimitScope::Account => config.tracking().accounts,
    }
}

const fn scope_discriminant(scope: RateLimitScope) -> u8 {
    match scope {
        RateLimitScope::ApiPrincipal => 0,
        RateLimitScope::Tenant => 1,
        RateLimitScope::Account => 2,
    }
}

fn decode_scope(value: u8) -> Result<RateLimitScope, LmdbRateLimitError> {
    match value {
        0 => Ok(RateLimitScope::ApiPrincipal),
        1 => Ok(RateLimitScope::Tenant),
        2 => Ok(RateLimitScope::Account),
        _ => Err(LmdbRateLimitError::CorruptRecord),
    }
}

fn array<const LENGTH: usize>(input: &[u8]) -> Result<[u8; LENGTH], LmdbRateLimitError> {
    input
        .try_into()
        .map_err(|_| LmdbRateLimitError::CorruptRecord)
}
