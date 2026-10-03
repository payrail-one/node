use merchant_checkout_core::{CheckoutDispatchState, CheckoutState};

use crate::{
    MerchantCheckoutStoreError,
    codec::{checkout_key, decode_record, order_key},
    store::LmdbMerchantCheckoutStore,
};

impl LmdbMerchantCheckoutStore {
    pub(crate) fn validate_indexes(&self) -> Result<(), MerchantCheckoutStoreError> {
        let transaction = self.env.read_txn()?;
        if self.metadata.get(&transaction, b"network")? != Some(self.network.as_bytes()) {
            return Err(MerchantCheckoutStoreError::WrongNetwork);
        }
        let mut record_count = 0_u64;
        let mut pending_count = 0_u64;
        for item in self.records.iter(&transaction)? {
            let (key, encoded) = item?;
            let checkout = decode_record(encoded)?;
            let definition = checkout.definition();
            let id_key = checkout_key(checkout.id());
            if definition.network != self.network
                || key != id_key
                || self
                    .order_owners
                    .get(&transaction, &order_key(definition))?
                    != Some(key)
            {
                return Err(MerchantCheckoutStoreError::CorruptRecord);
            }
            record_count = record_count
                .checked_add(1)
                .ok_or(MerchantCheckoutStoreError::CorruptRecord)?;
            if matches!(
                checkout.state(),
                CheckoutState::Claimed {
                    dispatch: CheckoutDispatchState::Pending,
                    ..
                }
            ) {
                if self.pending_dispatch.get(&transaction, key)? != Some(()) {
                    return Err(MerchantCheckoutStoreError::CorruptRecord);
                }
                pending_count = pending_count
                    .checked_add(1)
                    .ok_or(MerchantCheckoutStoreError::CorruptRecord)?;
            }
        }
        if self.order_owners.len(&transaction)? != record_count
            || self.pending_dispatch.len(&transaction)? != pending_count
        {
            return Err(MerchantCheckoutStoreError::CorruptRecord);
        }
        Ok(())
    }
}
