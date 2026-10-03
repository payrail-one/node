use std::collections::BTreeMap;

use ledger_core::AccountId;

use crate::{
    PaymentIdempotencyStoreError,
    codec::{account_nonce_key, decode_record, request_key},
    store::LmdbPaymentIdempotencyStore,
    work_codec::{decode_schedule, due_key},
};

impl LmdbPaymentIdempotencyStore {
    pub(crate) fn validate_indexes(&self) -> Result<(), PaymentIdempotencyStoreError> {
        let transaction = self.env.read_txn()?;
        if self.metadata.get(&transaction, b"network")? != Some(self.network.as_bytes().as_slice())
        {
            return Err(PaymentIdempotencyStoreError::WrongNetwork);
        }
        self.validate_coordination(&transaction)?;
        let mut nonce_count = 0_u64;
        let mut operation_count = 0_u64;
        let mut reconciliation_count = 0_u64;
        let mut maximum_nonces = BTreeMap::<AccountId, u64>::new();
        for item in self.records.iter(&transaction)? {
            let (key, encoded) = item?;
            let reservation = decode_record(encoded)?;
            let intent = reservation.intent();
            if intent.network != self.network || key != request_key(intent.request_id) {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            if let Some(nonce) = reservation.state().nonce() {
                let owner = self.nonce_owners.get(
                    &transaction,
                    &account_nonce_key(intent.request_id.account, nonce),
                )?;
                if owner != Some(key) {
                    return Err(PaymentIdempotencyStoreError::CorruptRecord);
                }
                maximum_nonces
                    .entry(intent.request_id.account)
                    .and_modify(|maximum| *maximum = (*maximum).max(nonce))
                    .or_insert(nonce);
                nonce_count = nonce_count
                    .checked_add(1)
                    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            }
            if let Some(operation_id) = reservation.state().operation_id() {
                let owner = self
                    .operation_owners
                    .get(&transaction, operation_id.as_bytes().as_slice())?;
                if owner != Some(key) {
                    return Err(PaymentIdempotencyStoreError::CorruptRecord);
                }
                operation_count = operation_count
                    .checked_add(1)
                    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            }
            if reservation.state().requires_finality_reconciliation() {
                let schedule = self
                    .reconciliation_queue
                    .get(&transaction, key)?
                    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)
                    .and_then(decode_schedule)?;
                if self.reconciliation_due.get(
                    &transaction,
                    &due_key(schedule.next_attempt_at_ms, intent.request_id),
                )? != Some(())
                {
                    return Err(PaymentIdempotencyStoreError::CorruptRecord);
                }
                reconciliation_count = reconciliation_count
                    .checked_add(1)
                    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            }
        }
        if self.nonce_owners.len(&transaction)? != nonce_count
            || self.operation_owners.len(&transaction)? != operation_count
            || self.reconciliation_queue.len(&transaction)? != reconciliation_count
            || self.reconciliation_due.len(&transaction)? != reconciliation_count
            || self.account_next_nonces.len(&transaction)?
                != u64::try_from(maximum_nonces.len())
                    .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?
        {
            return Err(PaymentIdempotencyStoreError::CorruptRecord);
        }
        for (account, maximum) in maximum_nonces {
            let encoded = self
                .account_next_nonces
                .get(&transaction, account.as_bytes().as_slice())?
                .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            let next = u64::from_be_bytes(
                encoded
                    .try_into()
                    .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
            );
            if next <= maximum {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
        }
        Ok(())
    }
}
