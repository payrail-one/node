use std::ops::Bound;

use heed::RoTxn;
use ledger_core::NetworkId;
use payment_idempotency_core::{PaymentRequestId, ReconciliationPageLimit};
use payment_reconciliation_work_core::{
    DeferReason, LeaseAcquireOutcome, LeaseDuration, LeaseToken, ReconciliationLease,
    ReconciliationWorkError, ReconciliationWorkStore, ScheduledReconciliation, WorkerId,
};

use crate::{
    PaymentIdempotencyStoreError,
    codec::request_key,
    store::LmdbPaymentIdempotencyStore,
    work_codec::{
        StoredSchedule, decode_due_key, decode_schedule, due_key, encode_schedule, maximum_due_key,
    },
};

const LEASE_KEY: &[u8] = b"reconciliation_worker_lease";
const FENCE_KEY: &[u8] = b"reconciliation_worker_fence";
const LEASE_LENGTH: usize = 48;

impl ReconciliationWorkStore for LmdbPaymentIdempotencyStore {
    type Error = PaymentIdempotencyStoreError;

    fn network(&self) -> NetworkId {
        self.network()
    }

    fn acquire_lease(
        &self,
        owner: WorkerId,
        now_ms: u64,
        duration: LeaseDuration,
    ) -> Result<LeaseAcquireOutcome, Self::Error> {
        let mut transaction = self.env.write_txn()?;
        if let Some(existing) = self.read_lease(&transaction)? {
            if existing.is_active_at(now_ms) {
                return Ok(LeaseAcquireOutcome::Busy {
                    expires_at_ms: existing.expires_at_ms,
                });
            }
        }
        let previous = self.read_fence(&transaction)?;
        let token_value = previous
            .checked_add(1)
            .ok_or(ReconciliationWorkError::ArithmeticOverflow)?;
        let token =
            LeaseToken::new(token_value).ok_or(ReconciliationWorkError::ArithmeticOverflow)?;
        let expires_at_ms = now_ms
            .checked_add(duration.as_millis())
            .ok_or(ReconciliationWorkError::ArithmeticOverflow)?;
        let lease = ReconciliationLease {
            owner,
            token,
            expires_at_ms,
        };
        self.metadata
            .put(&mut transaction, FENCE_KEY, &token_value.to_be_bytes())?;
        self.metadata
            .put(&mut transaction, LEASE_KEY, &encode_lease(lease))?;
        transaction.commit()?;
        Ok(LeaseAcquireOutcome::Acquired(lease))
    }

    fn renew_lease(
        &self,
        lease: ReconciliationLease,
        now_ms: u64,
        duration: LeaseDuration,
    ) -> Result<ReconciliationLease, Self::Error> {
        let mut transaction = self.env.write_txn()?;
        self.require_active_lease(&transaction, lease, now_ms)?;
        let candidate = now_ms
            .checked_add(duration.as_millis())
            .ok_or(ReconciliationWorkError::ArithmeticOverflow)?;
        let renewed = ReconciliationLease {
            expires_at_ms: lease.expires_at_ms.max(candidate),
            ..lease
        };
        self.metadata
            .put(&mut transaction, LEASE_KEY, &encode_lease(renewed))?;
        transaction.commit()?;
        Ok(renewed)
    }

    fn release_lease(&self, lease: ReconciliationLease) -> Result<(), Self::Error> {
        let mut transaction = self.env.write_txn()?;
        if self.read_lease(&transaction)? != Some(lease) {
            return Err(ReconciliationWorkError::StaleLease.into());
        }
        self.metadata.delete(&mut transaction, LEASE_KEY)?;
        transaction.commit()?;
        Ok(())
    }

    fn due_work(
        &self,
        lease: ReconciliationLease,
        now_ms: u64,
        limit: ReconciliationPageLimit,
    ) -> Result<Vec<ScheduledReconciliation>, Self::Error> {
        let transaction = self.env.read_txn()?;
        self.require_active_lease(&transaction, lease, now_ms)?;
        let maximum = maximum_due_key(now_ms);
        let bounds = (
            Bound::<&[u8]>::Unbounded,
            Bound::Included(maximum.as_slice()),
        );
        let mut jobs = Vec::with_capacity(limit.get());
        for item in self
            .reconciliation_due
            .range(&transaction, &bounds)?
            .take(limit.get())
        {
            let (encoded_due, ()) = item?;
            let (next_attempt_at_ms, request_id) = decode_due_key(encoded_due)?;
            let schedule = self.required_schedule(&transaction, request_id)?;
            if schedule.next_attempt_at_ms != next_attempt_at_ms {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            let reservation = self
                .read_record(&transaction, request_id)?
                .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
            if !reservation.state().requires_finality_reconciliation() {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
            jobs.push(ScheduledReconciliation {
                request_id,
                request_digest: reservation.intent().request_digest,
                consecutive_failures: schedule.consecutive_failures,
                next_attempt_at_ms,
            });
        }
        Ok(jobs)
    }

    fn defer(
        &self,
        lease: ReconciliationLease,
        job: ScheduledReconciliation,
        reason: DeferReason,
        next_attempt_at_ms: u64,
        now_ms: u64,
    ) -> Result<ScheduledReconciliation, Self::Error> {
        if next_attempt_at_ms <= now_ms {
            return Err(ReconciliationWorkError::TimeNotFuture.into());
        }
        let mut transaction = self.env.write_txn()?;
        self.require_active_lease(&transaction, lease, now_ms)?;
        let current = self.required_schedule(&transaction, job.request_id)?;
        let reservation = self
            .read_record(&transaction, job.request_id)?
            .ok_or(ReconciliationWorkError::ScheduleChanged)?;
        if current.consecutive_failures != job.consecutive_failures
            || current.next_attempt_at_ms != job.next_attempt_at_ms
            || reservation.intent().request_digest != job.request_digest
            || !reservation.state().requires_finality_reconciliation()
        {
            return Err(ReconciliationWorkError::ScheduleChanged.into());
        }
        let consecutive_failures = match reason {
            DeferReason::Pending => 0,
            DeferReason::Error => current
                .consecutive_failures
                .checked_add(1)
                .ok_or(ReconciliationWorkError::ArithmeticOverflow)?,
        };
        if !self.reconciliation_due.delete(
            &mut transaction,
            &due_key(current.next_attempt_at_ms, job.request_id),
        )? {
            return Err(PaymentIdempotencyStoreError::CorruptRecord);
        }
        let updated = StoredSchedule {
            consecutive_failures,
            next_attempt_at_ms,
        };
        self.reconciliation_queue.put(
            &mut transaction,
            &request_key(job.request_id),
            &encode_schedule(updated),
        )?;
        self.reconciliation_due.put(
            &mut transaction,
            &due_key(next_attempt_at_ms, job.request_id),
            &(),
        )?;
        transaction.commit()?;
        Ok(ScheduledReconciliation {
            consecutive_failures,
            next_attempt_at_ms,
            ..job
        })
    }

    fn acknowledge_completed(
        &self,
        lease: ReconciliationLease,
        request_id: PaymentRequestId,
        now_ms: u64,
    ) -> Result<(), Self::Error> {
        let mut transaction = self.env.write_txn()?;
        self.require_active_lease(&transaction, lease, now_ms)?;
        let key = request_key(request_id);
        let Some(encoded) = self.reconciliation_queue.get(&transaction, &key)? else {
            return Ok(());
        };
        let reservation = self
            .read_record(&transaction, request_id)?
            .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
        if reservation.state().requires_finality_reconciliation() {
            return Err(ReconciliationWorkError::ScheduleChanged.into());
        }
        let schedule = decode_schedule(encoded)?;
        self.reconciliation_due.delete(
            &mut transaction,
            &due_key(schedule.next_attempt_at_ms, request_id),
        )?;
        self.reconciliation_queue.delete(&mut transaction, &key)?;
        transaction.commit()?;
        Ok(())
    }
}

impl LmdbPaymentIdempotencyStore {
    fn read_lease(
        &self,
        transaction: &RoTxn<'_>,
    ) -> Result<Option<ReconciliationLease>, PaymentIdempotencyStoreError> {
        self.metadata
            .get(transaction, LEASE_KEY)?
            .map(decode_lease)
            .transpose()
    }

    fn read_fence(&self, transaction: &RoTxn<'_>) -> Result<u64, PaymentIdempotencyStoreError> {
        self.metadata
            .get(transaction, FENCE_KEY)?
            .map(|encoded| {
                encoded
                    .try_into()
                    .map(u64::from_be_bytes)
                    .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)
            })
            .transpose()
            .map(|value| value.unwrap_or(0))
    }

    fn require_active_lease(
        &self,
        transaction: &RoTxn<'_>,
        lease: ReconciliationLease,
        now_ms: u64,
    ) -> Result<(), PaymentIdempotencyStoreError> {
        if self.read_lease(transaction)? == Some(lease) && lease.is_active_at(now_ms) {
            Ok(())
        } else {
            Err(ReconciliationWorkError::StaleLease.into())
        }
    }

    fn required_schedule(
        &self,
        transaction: &RoTxn<'_>,
        request_id: PaymentRequestId,
    ) -> Result<StoredSchedule, PaymentIdempotencyStoreError> {
        let encoded = self
            .reconciliation_queue
            .get(transaction, &request_key(request_id))?
            .ok_or(ReconciliationWorkError::ScheduleChanged)?;
        decode_schedule(encoded)
    }

    pub(crate) fn validate_coordination(
        &self,
        transaction: &RoTxn<'_>,
    ) -> Result<(), PaymentIdempotencyStoreError> {
        let fence = self.read_fence(transaction)?;
        if let Some(lease) = self.read_lease(transaction)? {
            if lease.token.get() != fence {
                return Err(PaymentIdempotencyStoreError::CorruptRecord);
            }
        }
        Ok(())
    }
}

fn encode_lease(lease: ReconciliationLease) -> [u8; LEASE_LENGTH] {
    let mut encoded = [0_u8; LEASE_LENGTH];
    encoded[..32].copy_from_slice(lease.owner.as_bytes());
    encoded[32..40].copy_from_slice(&lease.token.get().to_be_bytes());
    encoded[40..].copy_from_slice(&lease.expires_at_ms.to_be_bytes());
    encoded
}

fn decode_lease(encoded: &[u8]) -> Result<ReconciliationLease, PaymentIdempotencyStoreError> {
    if encoded.len() != LEASE_LENGTH {
        return Err(PaymentIdempotencyStoreError::CorruptRecord);
    }
    let owner = WorkerId::new(
        encoded[..32]
            .try_into()
            .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
    )
    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
    let token = LeaseToken::new(u64::from_be_bytes(
        encoded[32..40]
            .try_into()
            .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
    ))
    .ok_or(PaymentIdempotencyStoreError::CorruptRecord)?;
    let expires_at_ms = u64::from_be_bytes(
        encoded[40..]
            .try_into()
            .map_err(|_| PaymentIdempotencyStoreError::CorruptRecord)?,
    );
    Ok(ReconciliationLease {
        owner,
        token,
        expires_at_ms,
    })
}
