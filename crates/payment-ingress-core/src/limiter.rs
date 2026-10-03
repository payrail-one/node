use std::collections::{BTreeMap, BTreeSet};

use ledger_core::AccountId;
use payment_idempotency_core::TenantId;

use crate::{
    ApiPrincipalId, BucketPolicy, PaymentIngressRateLimiter, PaymentRateLimitConfig,
    RateLimitDecision, RateLimitError, RateLimitScope,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BucketState {
    tokens: u32,
    last_refill_ms: u64,
    expires_at_ms: u64,
}

#[derive(Clone, Copy, Debug)]
struct PreparedBucket<Key> {
    key: Key,
    previous_expiry: Option<u64>,
    next: BucketState,
}

#[derive(Clone, Copy, Debug)]
enum Preparation<Key> {
    Admitted(PreparedBucket<Key>),
    Limited(u64),
}

#[derive(Debug)]
struct BucketSet<Key> {
    scope: RateLimitScope,
    maximum_entries: usize,
    entries: BTreeMap<Key, BucketState>,
    expirations: BTreeSet<(u64, Key)>,
}

impl<Key: Copy + Ord> BucketSet<Key> {
    fn new(scope: RateLimitScope, maximum_entries: usize) -> Self {
        Self {
            scope,
            maximum_entries,
            entries: BTreeMap::new(),
            expirations: BTreeSet::new(),
        }
    }

    fn prune(&mut self, now_ms: u64) -> Result<(), RateLimitError> {
        let expired = self
            .expirations
            .iter()
            .take_while(|(expires_at_ms, _)| *expires_at_ms <= now_ms)
            .copied()
            .collect::<Vec<_>>();
        for (expires_at_ms, key) in &expired {
            if self.entries.get(key).map(|state| state.expires_at_ms) != Some(*expires_at_ms) {
                return Err(RateLimitError::AccountingInconsistent(self.scope));
            }
        }
        for expiration in expired {
            self.expirations.remove(&expiration);
            self.entries.remove(&expiration.1);
        }
        Ok(())
    }

    fn prepare(
        &self,
        key: Key,
        now_ms: u64,
        policy: BucketPolicy,
        idle_ttl_ms: u64,
    ) -> Result<Preparation<Key>, RateLimitError> {
        let (tokens, last_refill_ms, previous_expiry) =
            if let Some(current) = self.entries.get(&key) {
                if !self.expirations.contains(&(current.expires_at_ms, key)) {
                    return Err(RateLimitError::AccountingInconsistent(self.scope));
                }
                if now_ms < current.last_refill_ms {
                    return Err(RateLimitError::ClockRegression {
                        previous_ms: current.last_refill_ms,
                        now_ms,
                    });
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
                    u32::try_from(replenished).map_err(|_| RateLimitError::TimeOverflow)?,
                    advanced,
                    Some(current.expires_at_ms),
                )
            } else {
                if self.entries.len() >= self.maximum_entries {
                    return Err(RateLimitError::TrackingCapacityReached(self.scope));
                }
                (policy.capacity(), now_ms, None)
            };
        if tokens == 0 {
            let retry_at_ms = last_refill_ms
                .checked_add(policy.refill_interval_ms())
                .ok_or(RateLimitError::TimeOverflow)?;
            return Ok(Preparation::Limited(retry_at_ms));
        }
        let expires_at_ms = now_ms
            .checked_add(idle_ttl_ms)
            .ok_or(RateLimitError::TimeOverflow)?;
        Ok(Preparation::Admitted(PreparedBucket {
            key,
            previous_expiry,
            next: BucketState {
                tokens: tokens - 1,
                last_refill_ms,
                expires_at_ms,
            },
        }))
    }

    fn apply(&mut self, prepared: PreparedBucket<Key>) {
        if let Some(previous) = prepared.previous_expiry {
            self.expirations.remove(&(previous, prepared.key));
        }
        self.entries.insert(prepared.key, prepared.next);
        self.expirations
            .insert((prepared.next.expires_at_ms, prepared.key));
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug)]
pub struct BoundedPaymentRateLimiter {
    config: PaymentRateLimitConfig,
    principals: BucketSet<ApiPrincipalId>,
    tenants: BucketSet<TenantId>,
    accounts: BucketSet<AccountId>,
    last_observed_ms: Option<u64>,
}

impl BoundedPaymentRateLimiter {
    #[must_use]
    pub fn new(config: PaymentRateLimitConfig) -> Self {
        let tracking = config.tracking();
        Self {
            config,
            principals: BucketSet::new(RateLimitScope::ApiPrincipal, tracking.principals),
            tenants: BucketSet::new(RateLimitScope::Tenant, tracking.tenants),
            accounts: BucketSet::new(RateLimitScope::Account, tracking.accounts),
            last_observed_ms: None,
        }
    }

    #[must_use]
    pub fn tracked_identities(&self, scope: RateLimitScope) -> usize {
        match scope {
            RateLimitScope::ApiPrincipal => self.principals.len(),
            RateLimitScope::Tenant => self.tenants.len(),
            RateLimitScope::Account => self.accounts.len(),
        }
    }

    fn observe_time(&mut self, now_ms: u64) -> Result<(), RateLimitError> {
        if let Some(previous_ms) = self.last_observed_ms
            && now_ms < previous_ms
        {
            return Err(RateLimitError::ClockRegression {
                previous_ms,
                now_ms,
            });
        }
        self.last_observed_ms = Some(now_ms);
        Ok(())
    }

    fn admit_principal_attempt(
        &mut self,
        principal: ApiPrincipalId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, RateLimitError> {
        self.principals.prune(now_ms)?;
        let prepared = self.principals.prepare(
            principal,
            now_ms,
            self.config.principal(),
            self.config.idle_ttl_ms(),
        )?;
        match prepared {
            Preparation::Limited(retry_at_ms) => Ok(RateLimitDecision::Limited {
                scope: RateLimitScope::ApiPrincipal,
                retry_at_ms,
            }),
            Preparation::Admitted(prepared) => {
                self.principals.apply(prepared);
                Ok(RateLimitDecision::Admitted)
            }
        }
    }

    fn admit_new_identity(
        &mut self,
        tenant: TenantId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, RateLimitError> {
        self.tenants.prune(now_ms)?;
        self.accounts.prune(now_ms)?;

        let tenant = self.tenants.prepare(
            tenant,
            now_ms,
            self.config.tenant(),
            self.config.idle_ttl_ms(),
        )?;
        let account = self.accounts.prepare(
            account,
            now_ms,
            self.config.account(),
            self.config.idle_ttl_ms(),
        )?;

        if let Preparation::Limited(retry_at_ms) = tenant {
            return Ok(RateLimitDecision::Limited {
                scope: RateLimitScope::Tenant,
                retry_at_ms,
            });
        }
        if let Preparation::Limited(retry_at_ms) = account {
            return Ok(RateLimitDecision::Limited {
                scope: RateLimitScope::Account,
                retry_at_ms,
            });
        }

        let Preparation::Admitted(tenant) = tenant else {
            return Err(RateLimitError::AccountingInconsistent(
                RateLimitScope::Tenant,
            ));
        };
        let Preparation::Admitted(account) = account else {
            return Err(RateLimitError::AccountingInconsistent(
                RateLimitScope::Account,
            ));
        };
        self.tenants.apply(tenant);
        self.accounts.apply(account);
        Ok(RateLimitDecision::Admitted)
    }
}

impl PaymentIngressRateLimiter for BoundedPaymentRateLimiter {
    type Error = RateLimitError;

    fn admit_principal(
        &mut self,
        principal: ApiPrincipalId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error> {
        self.observe_time(now_ms)?;
        self.admit_principal_attempt(principal, now_ms)
    }

    fn admit_new_intent(
        &mut self,
        tenant: TenantId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error> {
        self.observe_time(now_ms)?;
        self.admit_new_identity(tenant, account, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PaymentRateLimitConfig, TrackingCapacities};

    fn limiter() -> BoundedPaymentRateLimiter {
        let policy = BucketPolicy::new(2, 100).unwrap();
        BoundedPaymentRateLimiter::new(
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
            .unwrap(),
        )
    }

    #[test]
    fn corrupt_account_index_cannot_partially_charge_tenant() {
        let tenant = TenantId::new([1; 32]);
        let account = AccountId::new([2; 32]);
        let mut limiter = limiter();
        assert_eq!(
            limiter.admit_new_intent(tenant, account, 0),
            Ok(RateLimitDecision::Admitted)
        );
        let tenant_before = limiter.tenants.entries.get(&tenant).copied().unwrap();
        let account_expiry = limiter
            .accounts
            .entries
            .get(&account)
            .map(|state| state.expires_at_ms)
            .unwrap();
        limiter
            .accounts
            .expirations
            .remove(&(account_expiry, account));

        assert_eq!(
            limiter.admit_new_intent(tenant, account, 1),
            Err(RateLimitError::AccountingInconsistent(
                RateLimitScope::Account
            ))
        );
        assert_eq!(
            limiter.tenants.entries.get(&tenant).copied(),
            Some(tenant_before)
        );
    }
}
