//! Infrastructure boundary for persistent Jellyfish Merkle Tree adapters.
//!
//! Storage crates implement the upstream `TreeReader` interface and use these
//! functions so hashing, bounds and mutation preparation remain authoritative
//! in this crate.

use jmt::{JellyfishMerkleTree, Version, storage::TreeReader, storage::TreeUpdateBatch};

use crate::{
    AuthenticatedStateError, AuthenticatedStateRoot, StateHasher, StateMutation, StateNamespace,
    StateProof, backend_error, state_key_hash,
    tree::{prepare_mutations, validate_key},
};

/// Prepares, but does not persist, one authenticated state version.
///
/// # Errors
///
/// Returns an error for invalid mutations or an unavailable/corrupt backend.
pub fn prepare_update<R: TreeReader>(
    reader: &R,
    version: Version,
    mutations: Vec<StateMutation>,
) -> Result<(AuthenticatedStateRoot, TreeUpdateBatch), AuthenticatedStateError> {
    let value_set = prepare_mutations(mutations)?;
    JellyfishMerkleTree::<_, StateHasher>::new(reader)
        .put_value_set(value_set, version)
        .map(|(root, update)| (root.into(), update))
        .map_err(|_| backend_error())
}

/// Reads a value and proof from a persistent authenticated-state backend.
///
/// # Errors
///
/// Returns an error for an invalid key or unavailable/corrupt tree version.
pub fn read_with_proof<R: TreeReader>(
    reader: &R,
    version: Version,
    namespace: StateNamespace,
    key: &[u8],
) -> Result<(Option<Vec<u8>>, StateProof), AuthenticatedStateError> {
    validate_key(key)?;
    JellyfishMerkleTree::<_, StateHasher>::new(reader)
        .get_with_proof(state_key_hash(namespace.value(), key), version)
        .map(|(value, proof)| (value, StateProof(proof)))
        .map_err(|_| backend_error())
}

/// Reads the root retained for one authenticated state version.
///
/// # Errors
///
/// Returns an error when the root is absent or the backend is corrupt.
pub fn read_root<R: TreeReader>(
    reader: &R,
    version: Version,
) -> Result<AuthenticatedStateRoot, AuthenticatedStateError> {
    JellyfishMerkleTree::<_, StateHasher>::new(reader)
        .get_root_hash(version)
        .map(Into::into)
        .map_err(|_| backend_error())
}
