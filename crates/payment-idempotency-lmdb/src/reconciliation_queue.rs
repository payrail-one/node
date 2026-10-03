use std::ops::Bound;

use heed::RwTxn;
use payment_idempotency_core::{
    PaymentRequestId, PaymentReservation, ReconciliationPage, ReconciliationPageLimit,
};

use crate::{
    PaymentIdempotencyStoreError,
    codec::{decode_record, decode_request_key, request_key},
    store::LmdbPaymentIdempotencyStore,
    work_codec::{StoredSchedule, decode_schedule, due_key, encode_schedule},
};

const QUEUE_READY_KEY: &[u8] = b"reconciliation_queue_ready";
const LEGACY_QUEUE_VERSION: &[u8] = &[1];
const QUEUE_VERSION: &[u8] = &[2];

impl LmdbPaymentIdempotencyStore {
    pub(crate) fn initialize_reconciliation_queue(
        &self,
    ) -> Result<(), PaymentIdempotencyStoreError> {
        let mut transaction = self.env.write_txn()?;
        match self.metadata.get(&transaction, QUEUE_READY_KEY)? {
            Some(value) if value == QUEUE_VERSION => return Ok(()),
            Some(value) if value == LEGACY_QUEUE_VERSION => {}
            Some(_) => return Err(PaymentIdempotencyStoreError::CorruptRecord),
            None => {}
        }

        let mut queued_keys = Vec::new();
        for item in self.records.iter(&transaction)? {
            let (key, encoded) = item?;
            let id = decode_request_key(key)?;
            let reservation = decode_record(encoded)?;
            if reservation.intent().network != self.network || reservation.intent().request_id != id
            {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            if reservation.state().requires_finality_reconciliation() {
                queued_keys.push((request_key(id), due_key(0, id)));
            }
        }

        self.reconciliation_queue.clear(&mut transaction)?;
        self.reconciliation_due.clear(&mut transaction)?;
        let schedule = encode_schedule(StoredSchedule::INITIAL);
        for (request, due) in queued_keys {
            self.reconciliation_queue
                .put(&mut transaction, &request, &schedule)?;
            self.reconciliation_due.put(&mut transaction, &due, &())?;
        }
        self.metadata
            .put(&mut transaction, QUEUE_READY_KEY, QUEUE_VERSION)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn sync_reconciliation_queue(
        &self,
        transaction: &mut RwTxn<'_>,
        key: &[u8],
        reservation: &PaymentReservation,
    ) -> Result<(), PaymentIdempotencyStoreError> {
        let existing = self.reconciliation_queue.get(transaction, key)?;
        if reservation.state().requires_finality_reconciliation() {
            if let Some(encoded) = existing {
                decode_schedule(encoded)?;
            } else {
                let schedule = encode_schedule(StoredSchedule::INITIAL);
                self.reconciliation_queue.put(transaction, key, &schedule)?;
                self.reconciliation_due.put(
                    transaction,
                    &due_key(0, reservation.intent().request_id),
                    &(),
                )?;
            }
        } else if let Some(encoded) = existing {
            let schedule = decode_schedule(encoded)?;
            self.reconciliation_due.delete(
                transaction,
                &due_key(schedule.next_attempt_at_ms, reservation.intent().request_id),
            )?;
            self.reconciliation_queue.delete(transaction, key)?;
        }
        Ok(())
    }

    /// Reads a bounded, ordered administrative page of unresolved submissions.
    ///
    /// # Errors
    ///
    /// Returns an error for corrupt queue ownership or an LMDB failure.
    pub fn scan_reconcilable_after(
        &self,
        after: Option<PaymentRequestId>,
        limit: ReconciliationPageLimit,
    ) -> Result<ReconciliationPage, PaymentIdempotencyStoreError> {
        let transaction = self.env.read_txn()?;
        let start_key = after.map(request_key);
        let bounds = (
            start_key
                .as_ref()
                .map_or(Bound::Unbounded, |key| Bound::Excluded(key.as_slice())),
            Bound::<&[u8]>::Unbounded,
        );
        let mut reservations = Vec::with_capacity(limit.get().saturating_add(1));
        for item in self
            .reconciliation_queue
            .range(&transaction, &bounds)?
            .take(limit.get().saturating_add(1))
        {
            let (key, schedule) = item?;
            let id = decode_request_key(key)?;
            let schedule = decode_schedule(schedule)?;
            if self
                .reconciliation_due
                .get(&transaction, &due_key(schedule.next_attempt_at_ms, id))?
                != Some(())
            {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            let reservation = self
                .read_record(&transaction, id)?
                .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            if !reservation.state().requires_finality_reconciliation() {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            reservations.push(reservation);
        }

        let has_more = reservations.len() > limit.get();
        if has_more {
            reservations.pop();
        }
        let next_cursor = if has_more {
            Some(
                reservations
                    .last()
                    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?
                    .intent()
                    .request_id,
            )
        } else {
            None
        };
        Ok(ReconciliationPage {
            reservations,
            next_cursor,
        })
    }
}
