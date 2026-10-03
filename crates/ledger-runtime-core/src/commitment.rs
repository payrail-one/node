use sha2::{Digest, Sha256};
use state_sync_core::{BlockHash, StateRoot};

const BLOCK_HASH_DOMAIN: &[u8] = b"ledger.block-hash.v1\0";
const STATE_ROOT_DOMAIN: &[u8] = b"ledger.state-root.v1\0";

#[must_use]
pub fn block_hash(parent: BlockHash, canonical_payload: &[u8]) -> BlockHash {
    let mut hasher = Sha256::new();
    hasher.update(BLOCK_HASH_DOMAIN);
    hasher.update(parent.as_bytes());
    hasher.update(canonical_payload);
    BlockHash::new(hasher.finalize().into())
}

#[must_use]
pub fn state_root(canonical_state: &[u8]) -> StateRoot {
    let mut hasher = Sha256::new();
    hasher.update(STATE_ROOT_DOMAIN);
    hasher.update(canonical_state);
    StateRoot::new(hasher.finalize().into())
}
