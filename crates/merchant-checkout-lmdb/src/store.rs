use std::{fs, ops::Bound, path::Path};

use heed::{
    Database, Env, RoTxn, RwTxn,
    types::{Bytes, Unit},
};
use ledger_core::{AccountId, Balance, NetworkId};
use merchant_checkout_core::{
    Checkout, CheckoutDefinition, CheckoutDispatchState, CheckoutError, CheckoutId,
    CheckoutMutation, CheckoutMutationOutcome, CheckoutState, PendingCheckoutPage,
};
use payment_idempotency_core::{ReconciliationPageLimit, TenantId};

use crate::{
    MINIMUM_MAP_SIZE, MerchantCheckoutStoreError, MerchantCheckoutStoreOptions,
    codec::{checkout_key, decode_checkout_key, decode_record, encode_record, order_key},
    environment::open_environment,
};

const NETWORK_KEY: &[u8] = b"network";

#[derive(Debug)]
pub struct LmdbMerchantCheckoutStore {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) records: Database<Bytes, Bytes>,
    pub(crate) order_owners: Database<Bytes, Bytes>,
    pub(crate) pending_dispatch: Database<Bytes, Unit>,
    pub(crate) network: NetworkId,
}

impl LmdbMerchantCheckoutStore {
    /// Opens a network-bound checkout store with LMDB locking and full default
    /// durability.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths, invalid options, another network,
    /// corrupt indexes or an LMDB failure.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: MerchantCheckoutStoreOptions,
    ) -> Result<Self, MerchantCheckoutStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(MerchantCheckoutStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(MerchantCheckoutStoreError::UnsafePath);
        }
        let path = fs::canonicalize(requested)?;
        let env = open_environment(&path, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let records = env.create_database(&mut transaction, Some("records"))?;
        let order_owners = env.create_database(&mut transaction, Some("order_owners"))?;
        let pending_dispatch = env.create_database(&mut transaction, Some("pending_dispatch"))?;
        match metadata.get(&transaction, NETWORK_KEY)? {
            Some(stored) if stored != network.as_bytes() => {
                return Err(MerchantCheckoutStoreError::WrongNetwork);
            }
            Some(_) => {}
            None => metadata.put(&mut transaction, NETWORK_KEY, network.as_bytes().as_slice())?,
        }
        transaction.commit()?;
        let store = Self {
            env,
            metadata,
            records,
            order_owners,
            pending_dispatch,
            network,
        };
        store.validate_indexes()?;
        Ok(store)
    }

    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Creates an immutable checkout or returns the exact existing checkout.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, invalid definition, conflicting
    /// order, corrupt index or LMDB failure.
    pub fn create(
        &self,
        definition: CheckoutDefinition,
        now_ms: u64,
    ) -> Result<CheckoutMutation, MerchantCheckoutStoreError> {
        if definition.network != self.network {
            return Err(MerchantCheckoutStoreError::WrongNetwork);
        }
        let checkout = Checkout::new(definition, now_ms)?;
        let id_key = checkout_key(checkout.id());
        let merchant_order_key = order_key(definition);
        let mut transaction = self.env.write_txn()?;
        if let Some(owner) = self.order_owners.get(&transaction, &merchant_order_key)? {
            if owner != id_key {
                return Err(MerchantCheckoutStoreError::CorruptRecord);
            }
            let existing = self.required_record(&transaction, checkout.id())?;
            if existing.definition() != definition {
                return Err(CheckoutError::DefinitionConflict.into());
            }
            return Ok(CheckoutMutation {
                outcome: CheckoutMutationOutcome::ExistingSame,
                checkout: existing,
            });
        }
        if self.records.get(&transaction, &id_key)?.is_some() {
            return Err(MerchantCheckoutStoreError::CorruptRecord);
        }
        self.put_record(&mut transaction, &id_key, &checkout)?;
        self.order_owners
            .put(&mut transaction, &merchant_order_key, &id_key)?;
        transaction.commit()?;
        Ok(CheckoutMutation {
            outcome: CheckoutMutationOutcome::Applied,
            checkout,
        })
    }

    /// Reads a checkout within the owning tenant boundary.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt data or an LMDB failure.
    pub fn get(
        &self,
        tenant: TenantId,
        id: CheckoutId,
    ) -> Result<Option<Checkout>, MerchantCheckoutStoreError> {
        let transaction = self.env.read_txn()?;
        let checkout = self.read_record(&transaction, id)?;
        Ok(checkout.filter(|stored| stored.definition().tenant == tenant))
    }

    /// Atomically claims an open checkout and queues gateway dispatch.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing checkout, tenant mismatch, invalid or
    /// conflicting claim, corrupt index or LMDB failure.
    pub fn claim(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        payer: AccountId,
        fee: Balance,
        now_ms: u64,
    ) -> Result<CheckoutMutation, MerchantCheckoutStoreError> {
        let key = checkout_key(id);
        let mut transaction = self.env.write_txn()?;
        let mut checkout = self.required_tenant_record(&transaction, tenant, id)?;
        let outcome = checkout.claim(payer, fee, now_ms)?;
        if outcome == CheckoutMutationOutcome::Applied {
            self.pending_dispatch.put(&mut transaction, &key, &())?;
            self.put_record(&mut transaction, &key, &checkout)?;
            transaction.commit()?;
        }
        Ok(CheckoutMutation { outcome, checkout })
    }

    /// Atomically records gateway ownership and removes pending dispatch work.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing checkout, tenant mismatch, invalid state,
    /// corrupt index or LMDB failure.
    pub fn mark_gateway_recorded(
        &self,
        tenant: TenantId,
        id: CheckoutId,
        now_ms: u64,
    ) -> Result<CheckoutMutation, MerchantCheckoutStoreError> {
        let key = checkout_key(id);
        let mut transaction = self.env.write_txn()?;
        let mut checkout = self.required_tenant_record(&transaction, tenant, id)?;
        let outcome = checkout.mark_gateway_recorded(now_ms)?;
        if outcome == CheckoutMutationOutcome::Applied {
            if !self.pending_dispatch.delete(&mut transaction, &key)? {
                return Err(MerchantCheckoutStoreError::CorruptRecord);
            }
            self.put_record(&mut transaction, &key, &checkout)?;
            transaction.commit()?;
        }
        Ok(CheckoutMutation { outcome, checkout })
    }

    /// Reads a bounded exclusive-cursor page of pending gateway dispatch work.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt data or an LMDB failure.
    pub fn pending_dispatch_after(
        &self,
        after: Option<CheckoutId>,
        limit: ReconciliationPageLimit,
    ) -> Result<PendingCheckoutPage, MerchantCheckoutStoreError> {
        let transaction = self.env.read_txn()?;
        let start = after.map(checkout_key);
        let bounds = (
            start
                .as_ref()
                .map_or(Bound::Unbounded, |key| Bound::Excluded(key.as_slice())),
            Bound::<&[u8]>::Unbounded,
        );
        let mut checkouts = Vec::with_capacity(limit.get().saturating_add(1));
        for item in self
            .pending_dispatch
            .range(&transaction, &bounds)?
            .take(limit.get().saturating_add(1))
        {
            let (key, ()) = item?;
            let id = decode_checkout_key(key)?;
            let checkout = self.required_record(&transaction, id)?;
            if !is_pending(checkout.state()) {
                return Err(MerchantCheckoutStoreError::CorruptRecord);
            }
            checkouts.push(checkout);
        }
        let has_more = checkouts.len() > limit.get();
        if has_more {
            checkouts.pop();
        }
        let next_cursor = if has_more {
            Some(
                checkouts
                    .last()
                    .ok_or(MerchantCheckoutStoreError::CorruptRecord)?
                    .id(),
            )
        } else {
            None
        };
        Ok(PendingCheckoutPage {
            checkouts,
            next_cursor,
        })
    }

    pub(crate) fn read_record(
        &self,
        transaction: &RoTxn<'_>,
        id: CheckoutId,
    ) -> Result<Option<Checkout>, MerchantCheckoutStoreError> {
        let key = checkout_key(id);
        let Some(encoded) = self.records.get(transaction, &key)? else {
            return Ok(None);
        };
        let checkout = decode_record(encoded)?;
        if checkout.definition().network != self.network || checkout.id() != id {
            return Err(MerchantCheckoutStoreError::CorruptRecord);
        }
        self.validate_record_indexes(transaction, &checkout)?;
        Ok(Some(checkout))
    }

    fn required_record(
        &self,
        transaction: &RoTxn<'_>,
        id: CheckoutId,
    ) -> Result<Checkout, MerchantCheckoutStoreError> {
        self.read_record(transaction, id)?
            .ok_or(MerchantCheckoutStoreError::CheckoutNotFound)
    }

    fn required_tenant_record(
        &self,
        transaction: &RoTxn<'_>,
        tenant: TenantId,
        id: CheckoutId,
    ) -> Result<Checkout, MerchantCheckoutStoreError> {
        let checkout = self.required_record(transaction, id)?;
        if checkout.definition().tenant == tenant {
            Ok(checkout)
        } else {
            Err(MerchantCheckoutStoreError::CheckoutNotFound)
        }
    }

    fn put_record(
        &self,
        transaction: &mut RwTxn<'_>,
        key: &[u8],
        checkout: &Checkout,
    ) -> Result<(), MerchantCheckoutStoreError> {
        self.records
            .put(transaction, key, &encode_record(checkout))?;
        Ok(())
    }

    fn validate_record_indexes(
        &self,
        transaction: &RoTxn<'_>,
        checkout: &Checkout,
    ) -> Result<(), MerchantCheckoutStoreError> {
        let key = checkout_key(checkout.id());
        if self
            .order_owners
            .get(transaction, &order_key(checkout.definition()))?
            != Some(key.as_slice())
        {
            return Err(MerchantCheckoutStoreError::CorruptRecord);
        }
        let dispatch_entry = self.pending_dispatch.get(transaction, &key)?;
        if (is_pending(checkout.state()) && dispatch_entry != Some(()))
            || (!is_pending(checkout.state()) && dispatch_entry.is_some())
        {
            return Err(MerchantCheckoutStoreError::CorruptRecord);
        }
        Ok(())
    }
}

pub(crate) fn is_pending(state: CheckoutState) -> bool {
    matches!(
        state,
        CheckoutState::Claimed {
            dispatch: CheckoutDispatchState::Pending,
            ..
        }
    )
}
