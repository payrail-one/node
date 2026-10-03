use std::ops::Bound;

use heed::{RoTxn, RwTxn};
use payment_ingress_core::RateLimitError;

use crate::{
    LmdbPaymentRateLimiter, LmdbRateLimitError,
    codec::{
        BucketState, decode_bucket, decode_expiration_key, decode_time, encode_bucket, encode_time,
        expiration_bounds, expiration_key, policy_for, tracking_capacity,
    },
    store::{LAST_OBSERVED_TIME_KEY, ScopeDatabase},
};

#[derive(Clone, Copy)]
pub(super) enum Preparation {
    Admitted {
        identity: [u8; 32],
        previous_expiry: Option<u64>,
        next: BucketState,
    },
    Limited(u64),
}

impl LmdbPaymentRateLimiter {
    pub(super) fn prepare(
        &self,
        transaction: &mut RwTxn<'_>,
        store: ScopeDatabase,
        identity: [u8; 32],
        now_ms: u64,
    ) -> Result<Preparation, LmdbRateLimitError> {
        let policy = policy_for(self.config, store.scope);
        let (tokens, last_refill_ms, previous_expiry) =
            if let Some(encoded) = store.buckets.get(transaction, &identity)? {
                let current = decode_bucket(encoded)?;
                self.validate_bucket_expiration(transaction, store, identity, current)?;
                if now_ms < current.last_refill_ms {
                    return Err(RateLimitError::ClockRegression {
                        previous_ms: current.last_refill_ms,
                        now_ms,
                    }
                    .into());
                }
                let elapsed = now_ms - current.last_refill_ms;
                let intervals = elapsed / policy.refill_interval_ms();
                let replenished = u64::from(current.tokens)
                    .saturating_add(intervals)
                    .min(u64::from(policy.capacity()));
                let advanced = intervals
                    .checked_mul(policy.refill_interval_ms())
                    .and_then(|duration| current.last_refill_ms.checked_add(duration))
                    .ok_or(RateLimitError::TimeOverflow)?;
                (
                    u32::try_from(replenished).map_err(|_| LmdbRateLimitError::IntegerOverflow)?,
                    advanced,
                    Some(current.expires_at_ms),
                )
            } else {
                self.ensure_capacity(transaction, store, now_ms)?;
                (policy.capacity(), now_ms, None)
            };
        if tokens == 0 {
            let retry_at_ms = last_refill_ms
                .checked_add(policy.refill_interval_ms())
                .ok_or(RateLimitError::TimeOverflow)?;
            return Ok(Preparation::Limited(retry_at_ms));
        }
        let expires_at_ms = now_ms
            .checked_add(self.config.idle_ttl_ms())
            .ok_or(RateLimitError::TimeOverflow)?;
        Ok(Preparation::Admitted {
            identity,
            previous_expiry,
            next: BucketState {
                tokens: tokens - 1,
                last_refill_ms,
                expires_at_ms,
            },
        })
    }

    pub(super) fn apply(
        &self,
        transaction: &mut RwTxn<'_>,
        store: ScopeDatabase,
        preparation: Preparation,
    ) -> Result<(), LmdbRateLimitError> {
        let Preparation::Admitted {
            identity,
            previous_expiry,
            next,
        } = preparation
        else {
            return Err(LmdbRateLimitError::CorruptRecord);
        };
        if let Some(previous_expiry) = previous_expiry
            && !self.expirations.delete(
                transaction,
                &expiration_key(store.scope, previous_expiry, &identity),
            )?
        {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        store
            .buckets
            .put(transaction, &identity, &encode_bucket(next))?;
        self.expirations.put(
            transaction,
            &expiration_key(store.scope, next.expires_at_ms, &identity),
            &(),
        )?;
        Ok(())
    }

    pub(super) fn observe_time(
        &self,
        transaction: &mut RwTxn<'_>,
        now_ms: u64,
    ) -> Result<u64, LmdbRateLimitError> {
        if let Some(encoded) = self.metadata.get(transaction, LAST_OBSERVED_TIME_KEY)? {
            let previous_ms = decode_time(encoded)?;
            if now_ms < previous_ms {
                return Ok(previous_ms);
            }
        }
        self.metadata
            .put(transaction, LAST_OBSERVED_TIME_KEY, &encode_time(now_ms))?;
        Ok(now_ms)
    }

    fn ensure_capacity(
        &self,
        transaction: &mut RwTxn<'_>,
        store: ScopeDatabase,
        now_ms: u64,
    ) -> Result<(), LmdbRateLimitError> {
        let maximum = tracking_capacity(self.config, store.scope);
        let count = usize::try_from(store.buckets.len(transaction)?)
            .map_err(|_| LmdbRateLimitError::IntegerOverflow)?;
        if count < maximum {
            return Ok(());
        }
        if count > maximum {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        self.remove_one_expired(transaction, store, now_ms)
    }

    fn remove_one_expired(
        &self,
        transaction: &mut RwTxn<'_>,
        store: ScopeDatabase,
        now_ms: u64,
    ) -> Result<(), LmdbRateLimitError> {
        let (minimum, maximum) = expiration_bounds(store.scope, now_ms);
        let bounds = (
            Bound::Included(minimum.as_slice()),
            Bound::Included(maximum.as_slice()),
        );
        let candidate = {
            let mut entries = self.expirations.range(transaction, &bounds)?;
            match entries.next() {
                Some(item) => {
                    let (encoded, ()) = item?;
                    Some(
                        <[u8; 41]>::try_from(encoded)
                            .map_err(|_| LmdbRateLimitError::CorruptRecord)?,
                    )
                }
                None => None,
            }
        }
        .ok_or(RateLimitError::TrackingCapacityReached(store.scope))?;
        let (scope, expires_at_ms, identity) = decode_expiration_key(&candidate)?;
        if scope != store.scope {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        let encoded = store
            .buckets
            .get(transaction, &identity)?
            .ok_or(LmdbRateLimitError::CorruptRecord)?;
        if decode_bucket(encoded)?.expires_at_ms != expires_at_ms {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        if !store.buckets.delete(transaction, &identity)?
            || !self.expirations.delete(transaction, &candidate)?
        {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        Ok(())
    }

    fn validate_bucket_expiration(
        &self,
        transaction: &RoTxn<'_>,
        store: ScopeDatabase,
        identity: [u8; 32],
        state: BucketState,
    ) -> Result<(), LmdbRateLimitError> {
        if self.expirations.get(
            transaction,
            &expiration_key(store.scope, state.expires_at_ms, &identity),
        )? != Some(())
        {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use ledger_core::NetworkId;
    use payment_ingress_core::{
        ApiPrincipalId, BucketPolicy, PaymentRateLimitConfig, RateLimitScope, TrackingCapacities,
    };

    use super::*;
    use crate::{LmdbRateLimitOptions, codec::decode_bucket};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn config() -> PaymentRateLimitConfig {
        let policy = BucketPolicy::new(2, 100).unwrap();
        PaymentRateLimitConfig::new(
            policy,
            policy,
            policy,
            TrackingCapacities {
                principals: 2,
                tenants: 2,
                accounts: 2,
            },
            200,
        )
        .unwrap()
    }

    #[test]
    fn corrupt_expiration_index_blocks_charge_and_reopen() {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "payment-rate-limit-corrupt-{}-{sequence}",
            std::process::id()
        ));
        let network = NetworkId::new([1; 32]);
        let principal = ApiPrincipalId::new([2; 32]);
        let limiter =
            LmdbPaymentRateLimiter::open(&path, network, config(), LmdbRateLimitOptions::default())
                .unwrap();
        limiter.admit_principal(principal, 0).unwrap();
        let state = {
            let transaction = limiter.env.read_txn().unwrap();
            decode_bucket(
                limiter
                    .principals
                    .get(&transaction, principal.as_bytes())
                    .unwrap()
                    .unwrap(),
            )
            .unwrap()
        };
        let mut transaction = limiter.env.write_txn().unwrap();
        limiter
            .expirations
            .delete(
                &mut transaction,
                &expiration_key(
                    RateLimitScope::ApiPrincipal,
                    state.expires_at_ms,
                    principal.as_bytes(),
                ),
            )
            .unwrap();
        transaction.commit().unwrap();

        assert_eq!(
            limiter.admit_principal(principal, 1),
            Err(LmdbRateLimitError::CorruptRecord)
        );
        assert_eq!(
            limiter.tracked_identities(RateLimitScope::ApiPrincipal),
            Ok(1)
        );
        drop(limiter);
        assert!(matches!(
            LmdbPaymentRateLimiter::open(&path, network, config(), LmdbRateLimitOptions::default(),),
            Err(LmdbRateLimitError::CorruptRecord)
        ));
        fs::remove_dir_all(path).unwrap();
    }
}
