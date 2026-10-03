use std::collections::BTreeSet;

use jmt::{JellyfishMerkleTree, KeyHash, RootHash, Version, storage::TreeUpdateBatch};

use crate::{
    AuthenticatedStateError, AuthenticatedStateRoot, MAX_STATE_CHANGES, MAX_STATE_KEY_BYTES,
    MAX_STATE_VALUE_BYTES, MemoryTreeStore, StateEntry, StateHasher, StateMutation, StateNamespace,
    StateProof, backend, backend_error, state_key_hash,
};

type PreparedMutations = Vec<(KeyHash, Option<Vec<u8>>)>;

/// Opaque, side-effect-free tree update prepared against one exact version.
pub struct PreparedAuthenticatedStateUpdate {
    version: Version,
    previous_version: Option<Version>,
    previous_root: Option<AuthenticatedStateRoot>,
    root: AuthenticatedStateRoot,
    update: TreeUpdateBatch,
}

impl PreparedAuthenticatedStateUpdate {
    #[must_use]
    pub const fn version(&self) -> Version {
        self.version
    }

    #[must_use]
    pub const fn root(&self) -> AuthenticatedStateRoot {
        self.root
    }
}

#[derive(Clone, Debug, Default)]
pub struct AuthenticatedStateTree {
    store: MemoryTreeStore,
    latest_version: Option<Version>,
}

impl AuthenticatedStateTree {
    /// Creates version zero from a bounded set of state entries.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid bounds, duplicate keys or a tree backend failure.
    pub fn create(
        entries: Vec<StateEntry>,
    ) -> Result<(Self, AuthenticatedStateRoot), AuthenticatedStateError> {
        let mutations = entries
            .into_iter()
            .map(|entry| StateMutation::set(entry.namespace, entry.key, entry.value))
            .collect();
        let mut tree = Self::default();
        let root = tree.apply(0, mutations)?;
        Ok((tree, root))
    }

    /// Applies one ordered version atomically and returns its authenticated root.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-sequential version, invalid bounds, duplicate
    /// keys, storage conflict or tree backend failure. The tree remains unchanged.
    pub fn apply(
        &mut self,
        version: Version,
        mutations: Vec<StateMutation>,
    ) -> Result<AuthenticatedStateRoot, AuthenticatedStateError> {
        let prepared = self.prepare(version, mutations)?;
        self.commit(prepared)
    }

    /// Prepares one sequential version without mutating retained state.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-sequential version, invalid mutation bounds
    /// or a corrupt tree backend.
    pub fn prepare(
        &self,
        version: Version,
        mutations: Vec<StateMutation>,
    ) -> Result<PreparedAuthenticatedStateUpdate, AuthenticatedStateError> {
        self.validate_version(version)?;
        let previous_root = self
            .latest_version
            .map(|latest| self.root(latest))
            .transpose()?;
        let (root, update) = backend::prepare_update(&self.store, version, mutations)?;
        Ok(PreparedAuthenticatedStateUpdate {
            version,
            previous_version: self.latest_version,
            previous_root,
            root,
            update,
        })
    }

    /// Atomically publishes an update prepared against the current tree.
    ///
    /// # Errors
    ///
    /// Returns an error if another version won first or retained state changed.
    pub fn commit(
        &mut self,
        prepared: PreparedAuthenticatedStateUpdate,
    ) -> Result<AuthenticatedStateRoot, AuthenticatedStateError> {
        let PreparedAuthenticatedStateUpdate {
            version,
            previous_version,
            previous_root,
            root,
            update,
        } = prepared;
        self.validate_version(version)?;
        if self.latest_version != previous_version {
            return Err(AuthenticatedStateError::InvalidVersion);
        }
        let current_root = self
            .latest_version
            .map(|latest| self.root(latest))
            .transpose()?;
        if current_root != previous_root {
            return Err(AuthenticatedStateError::StoreConflict);
        }
        self.store.apply(&update)?;
        self.latest_version = Some(version);
        Ok(root)
    }

    /// Reads a value and a proof at an retained state version.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid key or unavailable/corrupt tree version.
    pub fn get_with_proof(
        &self,
        version: Version,
        namespace: StateNamespace,
        key: &[u8],
    ) -> Result<(Option<Vec<u8>>, StateProof), AuthenticatedStateError> {
        validate_key(key)?;
        let key_hash = state_key_hash(namespace.value(), key);
        let jmt = JellyfishMerkleTree::<_, StateHasher>::new(&self.store);
        jmt.get_with_proof(key_hash, version)
            .map(|(value, proof)| (value, StateProof(proof)))
            .map_err(|_| backend_error())
    }

    /// Returns the root for a retained state version.
    ///
    /// # Errors
    ///
    /// Returns an error when the version does not exist or its nodes are corrupt.
    pub fn root(
        &self,
        version: Version,
    ) -> Result<AuthenticatedStateRoot, AuthenticatedStateError> {
        backend::read_root(&self.store, version)
    }

    #[must_use]
    pub const fn latest_version(&self) -> Option<Version> {
        self.latest_version
    }

    #[must_use]
    pub fn retained_node_count(&self) -> usize {
        self.store.node_count()
    }

    fn validate_version(&self, version: Version) -> Result<(), AuthenticatedStateError> {
        let expected = match self.latest_version {
            None => 0,
            Some(latest) => latest
                .checked_add(1)
                .ok_or(AuthenticatedStateError::InvalidVersion)?,
        };
        if version == expected {
            Ok(())
        } else {
            Err(AuthenticatedStateError::InvalidVersion)
        }
    }
}

impl StateProof {
    #[must_use]
    pub fn verifies(
        &self,
        root: AuthenticatedStateRoot,
        namespace: StateNamespace,
        key: &[u8],
        expected_value: Option<&[u8]>,
    ) -> bool {
        if validate_key(key).is_err() {
            return false;
        }
        self.0
            .verify(
                RootHash::from(*root.as_bytes()),
                state_key_hash(namespace.value(), key),
                expected_value,
            )
            .is_ok()
    }
}

pub(crate) fn prepare_mutations(
    mutations: Vec<StateMutation>,
) -> Result<PreparedMutations, AuthenticatedStateError> {
    if mutations.len() > MAX_STATE_CHANGES {
        return Err(AuthenticatedStateError::TooManyChanges);
    }
    let mut prepared = Vec::with_capacity(mutations.len());
    for mutation in mutations {
        validate_key(&mutation.key)?;
        if mutation
            .value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_STATE_VALUE_BYTES)
        {
            return Err(AuthenticatedStateError::ValueTooLarge);
        }
        prepared.push((
            state_key_hash(mutation.namespace.value(), &mutation.key),
            mutation.value,
        ));
    }
    prepared.sort_by_key(|(key, _)| *key);
    let mut unique = BTreeSet::new();
    if prepared.iter().any(|(key, _)| !unique.insert(*key)) {
        return Err(AuthenticatedStateError::DuplicateKey);
    }
    Ok(prepared)
}

pub(crate) fn validate_key(key: &[u8]) -> Result<(), AuthenticatedStateError> {
    if key.is_empty() {
        Err(AuthenticatedStateError::EmptyKey)
    } else if key.len() > MAX_STATE_KEY_BYTES {
        Err(AuthenticatedStateError::KeyTooLarge)
    } else {
        Ok(())
    }
}
