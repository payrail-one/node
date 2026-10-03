use jmt::SimpleHasher;
use sha2::{Digest, Sha256};

const STATE_HASH_DOMAIN: &[u8] = b"payment.authenticated-state.jmt.v1\0";
const STATE_KEY_DOMAIN: &[u8] = b"payment.authenticated-state.key.v1\0";

pub(crate) struct StateHasher(Sha256);

impl SimpleHasher for StateHasher {
    fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(STATE_HASH_DOMAIN);
        Self(hasher)
    }

    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    fn finalize(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

pub(crate) fn state_key_hash(namespace: u8, key: &[u8]) -> jmt::KeyHash {
    let mut hasher = Sha256::new();
    hasher.update(STATE_KEY_DOMAIN);
    hasher.update([namespace]);
    hasher.update(key);
    jmt::KeyHash(hasher.finalize().into())
}
