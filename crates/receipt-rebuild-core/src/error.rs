use ledger_runtime_core::RuntimeError;

#[derive(Debug)]
pub enum ReceiptRebuildError<SourceError, IndexError> {
    Source(SourceError),
    Index(IndexError),
    InvalidBase,
    IndexBaseMismatch,
    IndexAhead,
    SourceChainMismatch,
    Runtime(RuntimeError),
    CommitmentMismatch,
    MissingIndexedReceipt,
    IndexedReceiptMismatch,
    FinalCursorMismatch,
    ArithmeticOverflow,
}
