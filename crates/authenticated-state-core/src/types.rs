use crate::StateHasher;

pub const MAX_STATE_CHANGES: usize = 1_000_000;
pub const MAX_STATE_KEY_BYTES: usize = 64;
pub const MAX_STATE_VALUE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StateNamespace(u8);

impl StateNamespace {
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateEntry {
    pub namespace: StateNamespace,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl StateEntry {
    #[must_use]
    pub fn new(namespace: StateNamespace, key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            namespace,
            key,
            value,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateMutation {
    pub namespace: StateNamespace,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

impl StateMutation {
    #[must_use]
    pub fn set(namespace: StateNamespace, key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            namespace,
            key,
            value: Some(value),
        }
    }

    #[must_use]
    pub fn delete(namespace: StateNamespace, key: Vec<u8>) -> Self {
        Self {
            namespace,
            key,
            value: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedStateRoot([u8; 32]);

impl AuthenticatedStateRoot {
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<jmt::RootHash> for AuthenticatedStateRoot {
    fn from(value: jmt::RootHash) -> Self {
        Self(value.into())
    }
}

pub struct StateProof(pub(crate) jmt::proof::SparseMerkleProof<StateHasher>);

impl Clone for StateProof {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl core::fmt::Debug for StateProof {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_tuple("StateProof").field(&self.0).finish()
    }
}
