use std::collections::VecDeque;

use state_sync_core::FinalizedCheckpoint;

use crate::{
    DevnetError,
    model::{FinalizedBlockView, FinalizedTransactionView},
};

const MAX_RECENT_BLOCKS: usize = 25;
const MAX_RECENT_TRANSACTIONS: usize = 50;

#[derive(Debug)]
pub struct ExplorerIndex {
    tip: FinalizedCheckpoint,
    blocks: VecDeque<FinalizedBlockView>,
    transactions: VecDeque<FinalizedTransactionView>,
}

impl ExplorerIndex {
    pub fn new(genesis: FinalizedCheckpoint, genesis_view: FinalizedBlockView) -> Self {
        Self {
            tip: genesis,
            blocks: VecDeque::from([genesis_view]),
            transactions: VecDeque::new(),
        }
    }

    pub fn prepare_append(
        &self,
        previous: FinalizedCheckpoint,
        checkpoint: FinalizedCheckpoint,
        block: FinalizedBlockView,
        transaction: FinalizedTransactionView,
    ) -> Result<PreparedIndexAppend, DevnetError> {
        if previous != self.tip
            || checkpoint.height
                != previous
                    .height
                    .checked_add(1)
                    .ok_or(DevnetError::InternalInvariant)?
            || block.height != checkpoint.height.to_string()
            || transaction.block_height != block.height
        {
            return Err(DevnetError::InternalInvariant);
        }
        Ok(PreparedIndexAppend {
            previous,
            checkpoint,
            block,
            transaction,
        })
    }

    pub fn commit(&mut self, prepared: PreparedIndexAppend) -> Result<(), DevnetError> {
        if prepared.previous != self.tip {
            return Err(DevnetError::InternalInvariant);
        }
        self.tip = prepared.checkpoint;
        self.blocks.push_front(prepared.block);
        self.transactions.push_front(prepared.transaction);
        self.blocks.truncate(MAX_RECENT_BLOCKS);
        self.transactions.truncate(MAX_RECENT_TRANSACTIONS);
        Ok(())
    }

    pub fn blocks(&self) -> Vec<FinalizedBlockView> {
        self.blocks.iter().cloned().collect()
    }

    pub fn transactions(&self) -> Vec<FinalizedTransactionView> {
        self.transactions.iter().cloned().collect()
    }
}

pub struct PreparedIndexAppend {
    previous: FinalizedCheckpoint,
    checkpoint: FinalizedCheckpoint,
    block: FinalizedBlockView,
    transaction: FinalizedTransactionView,
}
