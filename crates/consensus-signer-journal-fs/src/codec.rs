use consensus_signer_core::{VoteIntent, VoteSlot, VoteStage};
use ledger_core::NetworkId;
use sha2::{Digest, Sha256};
use state_sync_core::BlockHash;

use crate::FileSigningJournalError;

const MAGIC: [u8; 8] = *b"SGNJNL01";
const BODY_LENGTH: usize = 97;
pub const RECORD_LENGTH: usize = BODY_LENGTH + 32;
pub const RECORD_LENGTH_U64: u64 = 129;

pub fn encode(intent: VoteIntent) -> [u8; RECORD_LENGTH] {
    let mut record = [0_u8; RECORD_LENGTH];
    record[..8].copy_from_slice(&MAGIC);
    record[8..40].copy_from_slice(intent.slot.network.as_bytes());
    record[40..48].copy_from_slice(&intent.slot.set_id.to_be_bytes());
    record[48..56].copy_from_slice(&intent.slot.round.to_be_bytes());
    record[56] = match intent.slot.stage {
        VoteStage::Proposal => 0,
        VoteStage::Prevote => 1,
        VoteStage::Precommit => 2,
    };
    record[57..65].copy_from_slice(&intent.slot.height.to_be_bytes());
    record[65..97].copy_from_slice(intent.target_hash.as_bytes());
    let digest: [u8; 32] = Sha256::digest(&record[..BODY_LENGTH]).into();
    record[BODY_LENGTH..].copy_from_slice(&digest);
    record
}

pub fn decode(record: &[u8]) -> Result<VoteIntent, FileSigningJournalError> {
    if record.len() != RECORD_LENGTH || record[..8] != MAGIC {
        return Err(FileSigningJournalError::CorruptRecord);
    }
    let expected: [u8; 32] = Sha256::digest(&record[..BODY_LENGTH]).into();
    if record[BODY_LENGTH..] != expected {
        return Err(FileSigningJournalError::CorruptRecord);
    }
    let stage = match record[56] {
        0 => VoteStage::Proposal,
        1 => VoteStage::Prevote,
        2 => VoteStage::Precommit,
        _ => return Err(FileSigningJournalError::CorruptRecord),
    };
    Ok(VoteIntent {
        slot: VoteSlot {
            network: NetworkId::new(array(&record[8..40])?),
            set_id: u64::from_be_bytes(array(&record[40..48])?),
            height: u64::from_be_bytes(array(&record[57..65])?),
            round: u64::from_be_bytes(array(&record[48..56])?),
            stage,
        },
        target_hash: BlockHash::new(array(&record[65..97])?),
    })
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], FileSigningJournalError> {
    bytes
        .try_into()
        .map_err(|_| FileSigningJournalError::CorruptRecord)
}
