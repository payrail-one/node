use std::{fs, path::Path};

use heed::{
    Database, Env, RwTxn,
    types::{Bytes, Unit},
};
use ledger_core::{AccountId, NetworkId};
use payment_idempotency_core::TenantId;
use payment_ingress_core::{
    ApiPrincipalId, PaymentIngressRateLimiter, PaymentRateLimitConfig, RateLimitDecision,
    RateLimitScope,
};

use crate::{
    LmdbRateLimitError, LmdbRateLimitOptions, MINIMUM_MAP_SIZE, admission::Preparation,
    codec::encode_config, environment::open_environment,
};

pub(crate) const NETWORK_KEY: &[u8] = b"network";
pub(crate) const CONFIG_KEY: &[u8] = b"configuration";
pub(crate) const LAST_OBSERVED_TIME_KEY: &[u8] = b"last_observed_time_ms";

#[derive(Clone, Copy)]
pub(crate) struct ScopeDatabase {
    pub(crate) scope: RateLimitScope,
    pub(crate) buckets: Database<Bytes, Bytes>,
}

#[derive(Debug)]
pub struct LmdbPaymentRateLimiter {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) principals: Database<Bytes, Bytes>,
    pub(crate) tenants: Database<Bytes, Bytes>,
    pub(crate) accounts: Database<Bytes, Bytes>,
    pub(crate) expirations: Database<Bytes, Unit>,
    pub(crate) network: NetworkId,
    pub(crate) config: PaymentRateLimitConfig,
}

impl LmdbPaymentRateLimiter {
    /// Opens a durable same-host multi-process rate limiter.
    ///
    /// The database is bound to one network and one exact policy. LMDB locking
    /// serializes all bucket changes across processes that open the same local
    /// directory.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths/options, network or policy mismatch,
    /// corrupt indexes, integer overflow or an LMDB failure.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        config: PaymentRateLimitConfig,
        options: LmdbRateLimitOptions,
    ) -> Result<Self, LmdbRateLimitError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(LmdbRateLimitError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(LmdbRateLimitError::UnsafePath);
        }
        let path = fs::canonicalize(requested)?;
        let env = open_environment(&path, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let principals = env.create_database(&mut transaction, Some("principals"))?;
        let tenants = env.create_database(&mut transaction, Some("tenants"))?;
        let accounts = env.create_database(&mut transaction, Some("accounts"))?;
        let expirations = env.create_database(&mut transaction, Some("expirations"))?;
        bind_network(&metadata, &mut transaction, network)?;
        bind_config(&metadata, &mut transaction, config)?;
        transaction.commit()?;

        let limiter = Self {
            env,
            metadata,
            principals,
            tenants,
            accounts,
            expirations,
            network,
            config,
        };
        limiter.validate()?;
        Ok(limiter)
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Charges one principal bucket in an atomic LMDB transaction.
    ///
    /// # Errors
    ///
    /// Returns an error for time regression, exhausted tracking capacity,
    /// corrupt accounting or storage failure.
    pub fn admit_principal(
        &self,
        principal: ApiPrincipalId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, LmdbRateLimitError> {
        let mut transaction = self.env.write_txn()?;
        let now_ms = self.observe_time(&mut transaction, now_ms)?;
        let store = self.scope_database(RateLimitScope::ApiPrincipal);
        let preparation = self.prepare(&mut transaction, store, *principal.as_bytes(), now_ms)?;
        let decision = match preparation {
            Preparation::Admitted { .. } => {
                self.apply(&mut transaction, store, preparation)?;
                RateLimitDecision::Admitted
            }
            Preparation::Limited(retry_at_ms) => RateLimitDecision::Limited {
                scope: RateLimitScope::ApiPrincipal,
                retry_at_ms,
            },
        };
        transaction.commit()?;
        Ok(decision)
    }

    /// Atomically charges tenant and account buckets for a new payment intent.
    ///
    /// Neither bucket is charged when either scope is limited or invalid.
    ///
    /// # Errors
    ///
    /// Returns an error for time regression, exhausted tracking capacity,
    /// corrupt accounting or storage failure.
    pub fn admit_new_intent(
        &self,
        tenant: TenantId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, LmdbRateLimitError> {
        let mut transaction = self.env.write_txn()?;
        let now_ms = self.observe_time(&mut transaction, now_ms)?;
        let tenant_store = self.scope_database(RateLimitScope::Tenant);
        let account_store = self.scope_database(RateLimitScope::Account);
        let tenant = self.prepare(&mut transaction, tenant_store, *tenant.as_bytes(), now_ms)?;
        let account = self.prepare(&mut transaction, account_store, *account.as_bytes(), now_ms)?;

        let decision = match (&tenant, &account) {
            (Preparation::Limited(retry_at_ms), _) => RateLimitDecision::Limited {
                scope: RateLimitScope::Tenant,
                retry_at_ms: *retry_at_ms,
            },
            (_, Preparation::Limited(retry_at_ms)) => RateLimitDecision::Limited {
                scope: RateLimitScope::Account,
                retry_at_ms: *retry_at_ms,
            },
            (Preparation::Admitted { .. }, Preparation::Admitted { .. }) => {
                self.apply(&mut transaction, tenant_store, tenant)?;
                self.apply(&mut transaction, account_store, account)?;
                RateLimitDecision::Admitted
            }
        };
        transaction.commit()?;
        Ok(decision)
    }

    /// Counts durable identities for one scope.
    ///
    /// # Errors
    ///
    /// Returns an error for integer conversion or storage failure.
    pub fn tracked_identities(&self, scope: RateLimitScope) -> Result<usize, LmdbRateLimitError> {
        let transaction = self.env.read_txn()?;
        usize::try_from(self.scope_database(scope).buckets.len(&transaction)?)
            .map_err(|_| LmdbRateLimitError::IntegerOverflow)
    }

    pub(crate) const fn scope_database(&self, scope: RateLimitScope) -> ScopeDatabase {
        let buckets = match scope {
            RateLimitScope::ApiPrincipal => self.principals,
            RateLimitScope::Tenant => self.tenants,
            RateLimitScope::Account => self.accounts,
        };
        ScopeDatabase { scope, buckets }
    }
}

impl PaymentIngressRateLimiter for LmdbPaymentRateLimiter {
    type Error = LmdbRateLimitError;

    fn admit_principal(
        &mut self,
        principal: ApiPrincipalId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error> {
        LmdbPaymentRateLimiter::admit_principal(self, principal, now_ms)
    }

    fn admit_new_intent(
        &mut self,
        tenant: TenantId,
        account: AccountId,
        now_ms: u64,
    ) -> Result<RateLimitDecision, Self::Error> {
        LmdbPaymentRateLimiter::admit_new_intent(self, tenant, account, now_ms)
    }
}

fn bind_network(
    metadata: &Database<Bytes, Bytes>,
    transaction: &mut RwTxn<'_>,
    network: NetworkId,
) -> Result<(), LmdbRateLimitError> {
    match metadata.get(transaction, NETWORK_KEY)? {
        Some(stored) if stored != network.as_bytes() => Err(LmdbRateLimitError::WrongNetwork),
        Some(_) => Ok(()),
        None => metadata
            .put(transaction, NETWORK_KEY, network.as_bytes())
            .map_err(Into::into),
    }
}

fn bind_config(
    metadata: &Database<Bytes, Bytes>,
    transaction: &mut RwTxn<'_>,
    config: PaymentRateLimitConfig,
) -> Result<(), LmdbRateLimitError> {
    let encoded = encode_config(config)?;
    match metadata.get(transaction, CONFIG_KEY)? {
        Some(stored) if stored != encoded => Err(LmdbRateLimitError::ConfigurationMismatch),
        Some(_) => Ok(()),
        None => metadata
            .put(transaction, CONFIG_KEY, &encoded)
            .map_err(Into::into),
    }
}
