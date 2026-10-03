use ledger_core::SignatureVerifier;
use ledger_runtime_core::{LedgerBlockExecutor, LedgerStateCodec};
use receipt_index_core::{
    FinalizedLedgerHistory, FinalizedReceiptBlock, ReceiptIndexBase, ReceiptIndexCursor,
    ReceiptIndexStore, ReceiptIndexWriteOutcome,
};

use crate::ReceiptRebuildError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptRebuildReport {
    pub base_height: u64,
    pub final_height: u64,
    pub replayed_blocks: u64,
    pub verified_existing_blocks: u64,
    pub newly_indexed_blocks: u64,
    pub newly_indexed_receipts: u64,
    pub final_operation_index: u64,
}

/// Replays finalized signed blocks from the verified recovery base and brings
/// an independent receipt index to the exact source cursor.
///
/// # Errors
///
/// Returns an error when source/index I/O fails, histories diverge, runtime
/// execution rejects a block, a commitment differs, or a stored receipt does
/// not equal the deterministic execution result.
pub fn rebuild_receipt_index<Source, Index, Verifier>(
    source: &Source,
    index: &Index,
    executor: &LedgerBlockExecutor<Verifier>,
) -> Result<ReceiptRebuildReport, ReceiptRebuildError<Source::Error, Index::Error>>
where
    Source: FinalizedLedgerHistory,
    Index: ReceiptIndexStore,
    Verifier: SignatureVerifier + Sync,
{
    let base = source
        .recovery_base()
        .map_err(ReceiptRebuildError::Source)?;
    let latest = source
        .latest_checkpoint()
        .map_err(ReceiptRebuildError::Source)?;
    if base.snapshot.network != executor.network()
        || latest.height < base.checkpoint.height
        || (latest.height == base.checkpoint.height && latest != base.checkpoint)
    {
        return Err(ReceiptRebuildError::InvalidBase);
    }
    let base_index = ReceiptIndexBase {
        network: base.snapshot.network,
        checkpoint: base.checkpoint,
        next_operation_index: base.snapshot.next_operation_index,
    };
    let initial_cursor = load_or_initialize(index, base_index)?;
    validate_index_base(initial_cursor, base_index, latest)?;

    let indexed_through = initial_cursor.latest().height;
    let mut state =
        LedgerStateCodec::encode(&base.snapshot).map_err(ReceiptRebuildError::Runtime)?;
    let mut previous = base.checkpoint;
    let mut report = ReceiptRebuildReport {
        base_height: base.checkpoint.height,
        final_height: latest.height,
        replayed_blocks: 0,
        verified_existing_blocks: 0,
        newly_indexed_blocks: 0,
        newly_indexed_receipts: 0,
        final_operation_index: base.snapshot.next_operation_index,
    };
    while previous.height < latest.height {
        let height = previous
            .height
            .checked_add(1)
            .ok_or(ReceiptRebuildError::ArithmeticOverflow)?;
        let archived = source
            .finalized_block(height)
            .map_err(ReceiptRebuildError::Source)?;
        if archived.previous != previous || archived.checkpoint.height != height {
            return Err(ReceiptRebuildError::SourceChainMismatch);
        }
        let executed = executor
            .execute(previous, &state, &archived.payload)
            .map_err(ReceiptRebuildError::Runtime)?;
        if executed.transition.commitment.block_hash != archived.checkpoint.block_hash
            || executed.transition.commitment.state_root != archived.checkpoint.state_root
        {
            return Err(ReceiptRebuildError::CommitmentMismatch);
        }
        let receipt_block = FinalizedReceiptBlock {
            network: base.snapshot.network,
            previous,
            checkpoint: archived.checkpoint,
            receipts: executed.receipts,
        };
        if height <= indexed_through {
            verify_existing(index, &receipt_block)?;
            report.verified_existing_blocks = report
                .verified_existing_blocks
                .checked_add(1)
                .ok_or(ReceiptRebuildError::ArithmeticOverflow)?;
        } else {
            let outcome = index
                .commit_finalized(&receipt_block)
                .map_err(ReceiptRebuildError::Index)?;
            if outcome == ReceiptIndexWriteOutcome::Committed {
                report.newly_indexed_blocks = report
                    .newly_indexed_blocks
                    .checked_add(1)
                    .ok_or(ReceiptRebuildError::ArithmeticOverflow)?;
                let receipt_count = u64::try_from(receipt_block.receipts.len())
                    .map_err(|_| ReceiptRebuildError::ArithmeticOverflow)?;
                report.newly_indexed_receipts = report
                    .newly_indexed_receipts
                    .checked_add(receipt_count)
                    .ok_or(ReceiptRebuildError::ArithmeticOverflow)?;
            }
        }
        state = executed.transition.state;
        previous = archived.checkpoint;
        report.replayed_blocks = report
            .replayed_blocks
            .checked_add(1)
            .ok_or(ReceiptRebuildError::ArithmeticOverflow)?;
    }
    validate_final(index, base.snapshot.network, latest, &state, &mut report)?;
    Ok(report)
}

fn load_or_initialize<SourceError, Index: ReceiptIndexStore>(
    index: &Index,
    base: ReceiptIndexBase,
) -> Result<ReceiptIndexCursor, ReceiptRebuildError<SourceError, Index::Error>> {
    if let Some(cursor) = index.cursor().map_err(ReceiptRebuildError::Index)? {
        Ok(cursor)
    } else {
        index.initialize(base).map_err(ReceiptRebuildError::Index)?;
        Ok(ReceiptIndexCursor::new(base))
    }
}

fn validate_index_base<SourceError, IndexError>(
    cursor: ReceiptIndexCursor,
    base: ReceiptIndexBase,
    latest: state_sync_core::FinalizedCheckpoint,
) -> Result<(), ReceiptRebuildError<SourceError, IndexError>> {
    if cursor.network() != base.network
        || cursor.base() != base.checkpoint
        || cursor.base_operation_index() != base.next_operation_index
    {
        return Err(ReceiptRebuildError::IndexBaseMismatch);
    }
    if cursor.latest().height > latest.height {
        return Err(ReceiptRebuildError::IndexAhead);
    }
    Ok(())
}

fn verify_existing<SourceError, Index: ReceiptIndexStore>(
    index: &Index,
    block: &FinalizedReceiptBlock,
) -> Result<(), ReceiptRebuildError<SourceError, Index::Error>> {
    for expected in &block.receipts {
        let stored = index
            .receipt_by_operation_id(expected.operation_id)
            .map_err(ReceiptRebuildError::Index)?
            .ok_or(ReceiptRebuildError::MissingIndexedReceipt)?;
        if stored != *expected {
            return Err(ReceiptRebuildError::IndexedReceiptMismatch);
        }
    }
    Ok(())
}

fn validate_final<SourceError, Index: ReceiptIndexStore>(
    index: &Index,
    network: ledger_core::NetworkId,
    latest: state_sync_core::FinalizedCheckpoint,
    state: &[u8],
    report: &mut ReceiptRebuildReport,
) -> Result<(), ReceiptRebuildError<SourceError, Index::Error>> {
    let snapshot =
        LedgerStateCodec::decode(network, state).map_err(ReceiptRebuildError::Runtime)?;
    let cursor = index
        .cursor()
        .map_err(ReceiptRebuildError::Index)?
        .ok_or(ReceiptRebuildError::FinalCursorMismatch)?;
    if cursor.latest() != latest || cursor.next_operation_index() != snapshot.next_operation_index {
        return Err(ReceiptRebuildError::FinalCursorMismatch);
    }
    report.final_operation_index = cursor.next_operation_index();
    Ok(())
}
