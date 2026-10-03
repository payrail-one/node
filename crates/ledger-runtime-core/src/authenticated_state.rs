use authenticated_state_core::{
    AuthenticatedStateError, AuthenticatedStateRoot, AuthenticatedStateTree, MAX_STATE_CHANGES,
    PreparedAuthenticatedStateUpdate, StateEntry, StateMutation, StateNamespace, StateProof,
};
use ledger_core::{LedgerError, LedgerSnapshot, NetworkId};
use state_sync_core::StateRoot;

use crate::{LedgerRow, LedgerStateRows, RuntimeError};

const SINGLETON_KEY: &[u8] = b"value";

/// Stable, consensus-visible namespaces for normalized ledger state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LedgerStateNamespace {
    Network = 0,
    RegistryAuthority = 1,
    Asset = 2,
    Balance = 3,
    Nonce = 4,
    OperationSequence = 5,
    AccountStatus = 6,
    Contract = 7,
    ContractState = 8,
}

impl LedgerStateNamespace {
    #[must_use]
    pub const fn state_namespace(self) -> StateNamespace {
        StateNamespace::new(self as u8)
    }
}

/// A proof bound to the ledger namespace, raw key and value it authenticates.
#[derive(Clone, Debug)]
pub struct LedgerStateProof {
    namespace: LedgerStateNamespace,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    proof: StateProof,
}

impl LedgerStateProof {
    #[must_use]
    pub fn from_authenticated_parts(
        namespace: LedgerStateNamespace,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        proof: StateProof,
    ) -> Self {
        Self {
            namespace,
            key,
            value,
            proof,
        }
    }

    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    #[must_use]
    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    #[must_use]
    pub const fn namespace(&self) -> LedgerStateNamespace {
        self.namespace
    }

    #[must_use]
    pub fn verifies(&self, root: StateRoot) -> bool {
        self.proof.verifies(
            AuthenticatedStateRoot::new(*root.as_bytes()),
            self.namespace.state_namespace(),
            &self.key,
            self.value.as_deref(),
        )
    }
}

/// Incremental authenticated view of canonical normalized ledger rows.
///
/// Tree version zero is the state at `base_height`; subsequent tree versions
/// are derived from the block-height offset. This permits a verified snapshot
/// at any block height to seed a new local tree without synthetic versions.
#[derive(Clone, Debug)]
pub struct LedgerAuthenticatedState {
    tree: AuthenticatedStateTree,
    rows: LedgerStateRows,
    base_height: u64,
    latest_height: u64,
    root: StateRoot,
}

/// Opaque normalized-ledger update prepared against one exact chain state.
///
/// Preparation performs all fallible snapshot normalization and authenticated
/// tree work without changing the live view. Only [`LedgerAuthenticatedState::commit`]
/// can publish the result.
pub struct PreparedLedgerAuthenticatedUpdate {
    height: u64,
    previous_height: u64,
    previous_root: StateRoot,
    root: StateRoot,
    rows: LedgerStateRows,
    tree_update: PreparedAuthenticatedStateUpdate,
}

impl PreparedLedgerAuthenticatedUpdate {
    #[must_use]
    pub const fn height(&self) -> u64 {
        self.height
    }

    #[must_use]
    pub const fn root(&self) -> StateRoot {
        self.root
    }
}

impl LedgerAuthenticatedState {
    /// Creates an authenticated tree from a validated snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for a wrong network, invalid snapshot or tree failure.
    pub fn create(
        network: NetworkId,
        base_height: u64,
        snapshot: &LedgerSnapshot,
    ) -> Result<Self, RuntimeError> {
        if snapshot.network != network {
            return Err(RuntimeError::InvalidSnapshot(LedgerError::WrongNetwork));
        }
        let rows = LedgerStateRows::from_snapshot(snapshot)?;
        let (tree, root) = AuthenticatedStateTree::create(authenticated_state_entries(&rows)?)?;
        Ok(Self {
            tree,
            rows,
            base_height,
            latest_height: base_height,
            root: state_root(root),
        })
    }

    /// Applies the next canonical snapshot as a bounded set of row mutations.
    ///
    /// The tree and current rows remain unchanged if validation or application
    /// fails.
    ///
    /// # Errors
    ///
    /// Returns an error for a gap, height overflow, changed network/registry
    /// identity, invalid snapshot or authenticated-tree failure.
    pub fn apply(
        &mut self,
        height: u64,
        snapshot: &LedgerSnapshot,
    ) -> Result<StateRoot, RuntimeError> {
        let prepared = self.prepare(height, snapshot)?;
        self.commit(prepared)
    }

    /// Prepares the next canonical snapshot without changing the live view.
    ///
    /// # Errors
    ///
    /// Returns an error for a gap, height overflow, changed network/registry
    /// identity, invalid snapshot or authenticated-tree failure.
    pub fn prepare(
        &self,
        height: u64,
        snapshot: &LedgerSnapshot,
    ) -> Result<PreparedLedgerAuthenticatedUpdate, RuntimeError> {
        let expected = self
            .latest_height
            .checked_add(1)
            .ok_or(RuntimeError::InvalidStateHeight)?;
        if height != expected {
            return Err(RuntimeError::InvalidStateHeight);
        }
        let next = LedgerStateRows::from_snapshot(snapshot)?;
        if next.network != self.rows.network
            || next.registry_authority != self.rows.registry_authority
        {
            return Err(RuntimeError::StateIdentityChanged);
        }
        let version = height
            .checked_sub(self.base_height)
            .ok_or(RuntimeError::InvalidStateHeight)?;
        let mutations = authenticated_state_delta(&self.rows, &next)?;
        let tree_update = self.tree.prepare(version, mutations)?;
        let root = state_root(tree_update.root());
        Ok(PreparedLedgerAuthenticatedUpdate {
            height,
            previous_height: self.latest_height,
            previous_root: self.root,
            root,
            rows: next,
            tree_update,
        })
    }

    /// Atomically publishes an update prepared against the current ledger view.
    ///
    /// # Errors
    ///
    /// Returns an error if another update won first or retained state changed.
    pub fn commit(
        &mut self,
        prepared: PreparedLedgerAuthenticatedUpdate,
    ) -> Result<StateRoot, RuntimeError> {
        if self.latest_height != prepared.previous_height || self.root != prepared.previous_root {
            return Err(RuntimeError::InvalidStateHeight);
        }
        self.tree.commit(prepared.tree_update)?;
        self.rows = prepared.rows;
        self.latest_height = prepared.height;
        self.root = prepared.root;
        Ok(prepared.root)
    }

    /// Returns a membership or non-membership proof for a retained height.
    ///
    /// # Errors
    ///
    /// Returns an error when the height or key is invalid or unavailable.
    pub fn prove(
        &self,
        height: u64,
        namespace: LedgerStateNamespace,
        key: &[u8],
    ) -> Result<LedgerStateProof, RuntimeError> {
        let version = self.version_at(height)?;
        let (value, proof) = self
            .tree
            .get_with_proof(version, namespace.state_namespace(), key)?;
        Ok(LedgerStateProof::from_authenticated_parts(
            namespace,
            key.to_vec(),
            value,
            proof,
        ))
    }

    /// Returns the authenticated root retained for a chain height.
    ///
    /// # Errors
    ///
    /// Returns an error when the height precedes the base or exceeds the latest.
    pub fn root_at(&self, height: u64) -> Result<StateRoot, RuntimeError> {
        self.tree
            .root(self.version_at(height)?)
            .map(state_root)
            .map_err(Into::into)
    }

    #[must_use]
    pub const fn root(&self) -> StateRoot {
        self.root
    }

    #[must_use]
    pub const fn base_height(&self) -> u64 {
        self.base_height
    }

    #[must_use]
    pub const fn latest_height(&self) -> u64 {
        self.latest_height
    }

    /// Confirms that an encoded predecessor represents this exact live view.
    pub(crate) fn validate_snapshot(
        &self,
        height: u64,
        snapshot: &LedgerSnapshot,
    ) -> Result<(), RuntimeError> {
        if height != self.latest_height {
            return Err(RuntimeError::InvalidStateHeight);
        }
        let rows = LedgerStateRows::from_snapshot(snapshot)?;
        if rows == self.rows {
            Ok(())
        } else {
            Err(RuntimeError::PreviousStateRootMismatch)
        }
    }

    fn version_at(&self, height: u64) -> Result<u64, RuntimeError> {
        if height > self.latest_height {
            return Err(RuntimeError::InvalidStateHeight);
        }
        height
            .checked_sub(self.base_height)
            .ok_or(RuntimeError::InvalidStateHeight)
    }
}

/// Rebuilds the authenticated root of one canonical ledger snapshot.
///
/// # Errors
///
/// Returns an error for an invalid snapshot or authenticated-tree failure.
pub fn authenticated_state_root(snapshot: &LedgerSnapshot) -> Result<StateRoot, RuntimeError> {
    LedgerAuthenticatedState::create(snapshot.network, 0, snapshot).map(|state| state.root())
}

/// Maps canonical normalized rows into bounded authenticated state entries.
///
/// # Errors
///
/// Returns an error when the aggregate state-entry bound is exceeded.
pub fn authenticated_state_entries(
    rows: &LedgerStateRows,
) -> Result<Vec<StateEntry>, RuntimeError> {
    let row_count = [
        rows.assets.len(),
        rows.balances.len(),
        rows.nonces.len(),
        rows.operation_sequence.len(),
        rows.account_statuses.len(),
        rows.contracts.len(),
        rows.contract_state.len(),
    ]
    .into_iter()
    .try_fold(2_usize, usize::checked_add)
    .ok_or(RuntimeError::AuthenticatedState(
        AuthenticatedStateError::TooManyChanges,
    ))?;
    if row_count > MAX_STATE_CHANGES {
        return Err(RuntimeError::AuthenticatedState(
            AuthenticatedStateError::TooManyChanges,
        ));
    }
    let mut entries = Vec::with_capacity(row_count);
    entries.extend([
        StateEntry::new(
            LedgerStateNamespace::Network.state_namespace(),
            SINGLETON_KEY.to_vec(),
            rows.network.as_bytes().to_vec(),
        ),
        StateEntry::new(
            LedgerStateNamespace::RegistryAuthority.state_namespace(),
            SINGLETON_KEY.to_vec(),
            rows.registry_authority.as_bytes().to_vec(),
        ),
    ]);
    append_entries(&mut entries, LedgerStateNamespace::Asset, &rows.assets);
    append_entries(&mut entries, LedgerStateNamespace::Balance, &rows.balances);
    append_entries(&mut entries, LedgerStateNamespace::Nonce, &rows.nonces);
    append_entries(
        &mut entries,
        LedgerStateNamespace::OperationSequence,
        &rows.operation_sequence,
    );
    append_entries(
        &mut entries,
        LedgerStateNamespace::Contract,
        &rows.contracts,
    );
    append_entries(
        &mut entries,
        LedgerStateNamespace::ContractState,
        &rows.contract_state,
    );
    append_entries(
        &mut entries,
        LedgerStateNamespace::AccountStatus,
        &rows.account_statuses,
    );
    Ok(entries)
}

fn append_entries(
    output: &mut Vec<StateEntry>,
    namespace: LedgerStateNamespace,
    rows: &[LedgerRow],
) {
    output.extend(rows.iter().map(|row| {
        StateEntry::new(
            namespace.state_namespace(),
            row.key.clone(),
            row.value.clone(),
        )
    }));
}

/// Computes a bounded deterministic authenticated-state delta between ledgers.
///
/// # Errors
///
/// Returns an error if network identity changes or the delta exceeds its bound.
pub fn authenticated_state_delta(
    previous: &LedgerStateRows,
    next: &LedgerStateRows,
) -> Result<Vec<StateMutation>, RuntimeError> {
    if next.network != previous.network || next.registry_authority != previous.registry_authority {
        return Err(RuntimeError::StateIdentityChanged);
    }
    let mut changes = Vec::new();
    diff_rows(
        &mut changes,
        LedgerStateNamespace::Asset,
        &previous.assets,
        &next.assets,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::Balance,
        &previous.balances,
        &next.balances,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::Nonce,
        &previous.nonces,
        &next.nonces,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::OperationSequence,
        &previous.operation_sequence,
        &next.operation_sequence,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::AccountStatus,
        &previous.account_statuses,
        &next.account_statuses,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::Contract,
        &previous.contracts,
        &next.contracts,
    )?;
    diff_rows(
        &mut changes,
        LedgerStateNamespace::ContractState,
        &previous.contract_state,
        &next.contract_state,
    )?;
    Ok(changes)
}

fn diff_rows(
    output: &mut Vec<StateMutation>,
    namespace: LedgerStateNamespace,
    previous: &[LedgerRow],
    next: &[LedgerRow],
) -> Result<(), RuntimeError> {
    let mut old_index = 0;
    let mut new_index = 0;
    while old_index < previous.len() || new_index < next.len() {
        match (previous.get(old_index), next.get(new_index)) {
            (Some(old), Some(new)) if old.key == new.key => {
                if old.value != new.value {
                    push_mutation(
                        output,
                        StateMutation::set(
                            namespace.state_namespace(),
                            new.key.clone(),
                            new.value.clone(),
                        ),
                    )?;
                }
                old_index += 1;
                new_index += 1;
            }
            (Some(old), Some(new)) if old.key < new.key => {
                push_mutation(
                    output,
                    StateMutation::delete(namespace.state_namespace(), old.key.clone()),
                )?;
                old_index += 1;
            }
            (Some(_) | None, Some(new)) => {
                push_mutation(
                    output,
                    StateMutation::set(
                        namespace.state_namespace(),
                        new.key.clone(),
                        new.value.clone(),
                    ),
                )?;
                new_index += 1;
            }
            (Some(old), None) => {
                push_mutation(
                    output,
                    StateMutation::delete(namespace.state_namespace(), old.key.clone()),
                )?;
                old_index += 1;
            }
            (None, None) => break,
        }
    }
    Ok(())
}

fn push_mutation(
    output: &mut Vec<StateMutation>,
    mutation: StateMutation,
) -> Result<(), RuntimeError> {
    if output.len() == MAX_STATE_CHANGES {
        return Err(RuntimeError::AuthenticatedState(
            AuthenticatedStateError::TooManyChanges,
        ));
    }
    output.push(mutation);
    Ok(())
}

const fn state_root(root: AuthenticatedStateRoot) -> StateRoot {
    StateRoot::new(*root.as_bytes())
}
