use payment_ingress_core::RateLimitScope;

use crate::{
    LmdbPaymentRateLimiter, LmdbRateLimitError,
    codec::{
        decode_bucket, decode_expiration_key, decode_time, encode_config, expiration_key,
        policy_for, tracking_capacity,
    },
    store::{CONFIG_KEY, LAST_OBSERVED_TIME_KEY, NETWORK_KEY},
};

impl LmdbPaymentRateLimiter {
    pub(crate) fn validate(&self) -> Result<(), LmdbRateLimitError> {
        let transaction = self.env.read_txn()?;
        if self.metadata.get(&transaction, NETWORK_KEY)? != Some(self.network.as_bytes()) {
            return Err(LmdbRateLimitError::WrongNetwork);
        }
        if self.metadata.get(&transaction, CONFIG_KEY)? != Some(&encode_config(self.config)?) {
            return Err(LmdbRateLimitError::ConfigurationMismatch);
        }
        let last_observed_ms = self
            .metadata
            .get(&transaction, LAST_OBSERVED_TIME_KEY)?
            .map(decode_time)
            .transpose()?;

        let mut expected_expirations = 0_u64;
        for scope in [
            RateLimitScope::ApiPrincipal,
            RateLimitScope::Tenant,
            RateLimitScope::Account,
        ] {
            let store = self.scope_database(scope);
            let count = store.buckets.len(&transaction)?;
            if count
                > u64::try_from(tracking_capacity(self.config, scope))
                    .map_err(|_| LmdbRateLimitError::IntegerOverflow)?
            {
                return Err(LmdbRateLimitError::CorruptRecord);
            }
            expected_expirations = expected_expirations
                .checked_add(count)
                .ok_or(LmdbRateLimitError::IntegerOverflow)?;
            for item in store.buckets.iter(&transaction)? {
                let (identity, encoded) = item?;
                let identity = <[u8; 32]>::try_from(identity)
                    .map_err(|_| LmdbRateLimitError::CorruptRecord)?;
                let state = decode_bucket(encoded)?;
                if state.tokens > policy_for(self.config, scope).capacity()
                    || state.expires_at_ms < state.last_refill_ms
                    || last_observed_ms.is_some_and(|last| state.last_refill_ms > last)
                    || self.expirations.get(
                        &transaction,
                        &expiration_key(scope, state.expires_at_ms, &identity),
                    )? != Some(())
                {
                    return Err(LmdbRateLimitError::CorruptRecord);
                }
            }
        }
        if self.expirations.len(&transaction)? != expected_expirations {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        if expected_expirations > 0 && last_observed_ms.is_none() {
            return Err(LmdbRateLimitError::CorruptRecord);
        }
        for item in self.expirations.iter(&transaction)? {
            let (key, ()) = item?;
            let (scope, expires_at_ms, identity) = decode_expiration_key(key)?;
            let encoded = self
                .scope_database(scope)
                .buckets
                .get(&transaction, &identity)?
                .ok_or(LmdbRateLimitError::CorruptRecord)?;
            if decode_bucket(encoded)?.expires_at_ms != expires_at_ms {
                return Err(LmdbRateLimitError::CorruptRecord);
            }
        }
        Ok(())
    }
}
