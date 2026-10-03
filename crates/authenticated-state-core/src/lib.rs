#![forbid(unsafe_code)]

pub mod backend;
mod error;
mod hasher;
mod store;
mod tree;
mod types;

pub use error::AuthenticatedStateError;
pub use tree::{AuthenticatedStateTree, PreparedAuthenticatedStateUpdate};
pub use types::{
    AuthenticatedStateRoot, MAX_STATE_CHANGES, MAX_STATE_KEY_BYTES, MAX_STATE_VALUE_BYTES,
    StateEntry, StateMutation, StateNamespace, StateProof,
};

pub(crate) use hasher::{StateHasher, state_key_hash};
pub(crate) use store::{MemoryTreeStore, backend_error};
