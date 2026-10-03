use payment_idempotency_core::PaymentIdempotencyError;
use payment_reconciliation_work_core::ReconciliationWorkError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentIdempotencyStoreError {
    Domain(PaymentIdempotencyError),
    Database,
    UnsafePath,
    MapSizeTooSmall,
    MapFull,
    WrongNetwork,
    RequestNotFound,
    DuplicateNonce,
    DuplicateOperation,
    Work(ReconciliationWorkError),
    CorruptRecord,
}

impl From<ReconciliationWorkError> for PaymentIdempotencyStoreError {
    fn from(error: ReconciliationWorkError) -> Self {
        Self::Work(error)
    }
}

impl From<PaymentIdempotencyError> for PaymentIdempotencyStoreError {
    fn from(error: PaymentIdempotencyError) -> Self {
        Self::Domain(error)
    }
}

impl From<heed::Error> for PaymentIdempotencyStoreError {
    fn from(error: heed::Error) -> Self {
        match error {
            heed::Error::Mdb(heed::MdbError::MapFull) => Self::MapFull,
            heed::Error::Mdb(
                heed::MdbError::PageNotFound
                | heed::MdbError::Corrupted
                | heed::MdbError::Panic
                | heed::MdbError::VersionMismatch
                | heed::MdbError::Invalid,
            ) => Self::CorruptRecord,
            _ => Self::Database,
        }
    }
}

impl From<std::io::Error> for PaymentIdempotencyStoreError {
    fn from(_error: std::io::Error) -> Self {
        Self::Database
    }
}
