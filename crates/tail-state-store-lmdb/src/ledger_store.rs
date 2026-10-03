use ledger_core::LedgerSnapshot;
use ledger_runtime_core::{
    LedgerStateCodec, LedgerStateRows, StateCommitmentScheme, authenticated_state_delta,
    authenticated_state_entries,
};
use state_sync_core::{FinalizedCheckpoint, SyncCompletion};
use tail_sync_core::VerifiedTailBlock;

use crate::{
    CommitOutcome, LmdbStateStoreError, StoredAuthenticatedState,
    codec::{
        BlockRecord, decode_block, encode_authenticated_state, encode_block,
        encode_commitment_policy, encode_current, encode_state, payload_hash,
    },
    rows::{sync_rows, write_rows},
    store::{
        AUTHENTICATED_STATE_KEY, BASE_KEY, COMMITMENT_POLICY_KEY, CURRENT_KEY, LmdbTailStateStore,
        PROTOCOL_CONFIG_DIGEST_KEY, REGISTRY_AUTHORITY_KEY, validate_next,
    },
};

impl LmdbTailStateStore {
    /// Initializes the state image, normalized rows and authenticated tree atomically.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid ledger state, a state-root mismatch, an
    /// initialized store or an LMDB failure.
    pub fn initialize_ledger_from_snapshot(
        &self,
        completion: SyncCompletion,
        snapshot: &LedgerSnapshot,
    ) -> Result<(), LmdbStateStoreError> {
        let config = self
            .verified_config
            .ok_or(LmdbStateStoreError::VerifiedNetworkConfigRequired)?;
        if completion.checkpoint != config.genesis_checkpoint() {
            return Err(LmdbStateStoreError::GenesisCheckpointMismatch);
        }
        let state = LedgerStateCodec::encode(snapshot)
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?;
        if snapshot.network != self.network || config.verify_genesis_state(&state).is_err() {
            return Err(LmdbStateStoreError::InvalidLedgerState);
        }
        let rows = LedgerStateRows::from_snapshot(snapshot)
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?;
        let mutations = authenticated_state_entries(&rows)
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?
            .into_iter()
            .map(|entry| {
                authenticated_state_core::StateMutation::set(
                    entry.namespace,
                    entry.key,
                    entry.value,
                )
            })
            .collect();
        let encoded_state = encode_state(&state)?;
        let encoded_checkpoint = encode_current(completion.checkpoint);
        let mut transaction = self.env.write_txn()?;
        if self.metadata.get(&transaction, BASE_KEY)?.is_some()
            || self.metadata.get(&transaction, CURRENT_KEY)?.is_some()
        {
            return Err(LmdbStateStoreError::AlreadyInitialized);
        }
        let (authenticated_root, authenticated_update) =
            self.authenticated_tree
                .prepare_update(&transaction, 0, mutations)?;
        let authenticated_state = StoredAuthenticatedState {
            base_height: completion.checkpoint.height,
            latest_height: completion.checkpoint.height,
            tree_version: 0,
            root: state_sync_core::StateRoot::new(*authenticated_root.as_bytes()),
        };
        self.require_checkpoint_matches_authenticated_root(
            completion.checkpoint,
            authenticated_state,
        )?;
        self.put_state_checked(&mut transaction, completion.checkpoint, &encoded_state)?;
        self.replace_ledger_rows(&mut transaction, &rows)?;
        self.authenticated_tree
            .apply_update(&mut transaction, &authenticated_update)?;
        self.metadata.put(
            &mut transaction,
            REGISTRY_AUTHORITY_KEY,
            rows.registry_authority.as_bytes().as_slice(),
        )?;
        self.metadata
            .put(&mut transaction, BASE_KEY, encoded_checkpoint.as_slice())?;
        self.metadata
            .put(&mut transaction, CURRENT_KEY, encoded_checkpoint.as_slice())?;
        let encoded_authenticated = encode_authenticated_state(authenticated_state);
        self.metadata.put(
            &mut transaction,
            AUTHENTICATED_STATE_KEY,
            encoded_authenticated.as_slice(),
        )?;
        let encoded_policy = encode_commitment_policy(self.commitment_policy);
        self.metadata.put(
            &mut transaction,
            COMMITMENT_POLICY_KEY,
            encoded_policy.as_slice(),
        )?;
        self.metadata.put(
            &mut transaction,
            PROTOCOL_CONFIG_DIGEST_KEY,
            config.protocol_digest().as_bytes().as_slice(),
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Atomically publishes rows, tree update, state image, block and cursor.
    ///
    /// # Errors
    ///
    /// Returns an error when prepared state is not a canonical ledger for this
    /// network, its root differs from the checkpoint, or commit fails.
    pub fn commit_verified_ledger(
        &self,
        verified: &VerifiedTailBlock,
    ) -> Result<CommitOutcome, LmdbStateStoreError> {
        if self
            .commitment_policy
            .scheme_at(verified.checkpoint().height)
            == StateCommitmentScheme::CanonicalStateV1
            && self
                .commitment_policy
                .root_for_state(verified.checkpoint().height, self.network, verified.state())
                .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?
                != verified.checkpoint().state_root
        {
            return Err(LmdbStateStoreError::InvalidLedgerState);
        }
        let snapshot = LedgerStateCodec::decode(self.network, verified.state())
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?;
        let rows = LedgerStateRows::from_snapshot(&snapshot)
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?;
        self.commit_verified_internal(verified, Some(&rows))
    }

    pub(crate) fn commit_verified_internal(
        &self,
        verified: &VerifiedTailBlock,
        ledger_rows: Option<&LedgerStateRows>,
    ) -> Result<CommitOutcome, LmdbStateStoreError> {
        if verified.network() != self.network {
            return Err(LmdbStateStoreError::WrongNetwork);
        }
        let requested = BlockRecord {
            network: verified.network(),
            previous: verified.previous(),
            checkpoint: verified.checkpoint(),
            payload_hash: payload_hash(verified.payload()),
        };
        let encoded_block = encode_block(&requested);
        let encoded_state = encode_state(verified.state())?;
        let block_key = requested.checkpoint.height.to_be_bytes();
        let mut transaction = self.env.write_txn()?;
        if ledger_rows.is_some() {
            self.require_network_config(&transaction)?;
        }
        let current = self.read_current(&transaction)?;

        if current == requested.checkpoint {
            let existing = self
                .blocks
                .get(&transaction, block_key.as_slice())?
                .ok_or(LmdbStateStoreError::ConflictingBlock)?;
            if decode_block(existing)? != requested {
                return Err(LmdbStateStoreError::ConflictingBlock);
            }
            if self
                .block_payloads
                .get(&transaction, block_key.as_slice())?
                != Some(verified.payload())
            {
                return Err(LmdbStateStoreError::ConflictingBlock);
            }
            self.require_same_state(&transaction, requested.checkpoint, &encoded_state)?;
            if let Some(expected) = ledger_rows {
                if self.read_ledger_rows(&transaction)? != *expected {
                    return Err(LmdbStateStoreError::ConflictingBlock);
                }
                self.require_authenticated_state(&transaction, requested.checkpoint.height)?;
            }
            return Ok(CommitOutcome::ExistingSame);
        }
        validate_next(current, &requested)?;
        if self
            .blocks
            .get(&transaction, block_key.as_slice())?
            .is_some()
            || self
                .block_payloads
                .get(&transaction, block_key.as_slice())?
                .is_some()
        {
            return Err(LmdbStateStoreError::ConflictingBlock);
        }
        let authenticated_update = if let Some(rows) = ledger_rows {
            Some(self.prepare_authenticated_transition(
                &transaction,
                current,
                requested.checkpoint.height,
                rows,
            )?)
        } else {
            if self
                .metadata
                .get(&transaction, AUTHENTICATED_STATE_KEY)?
                .is_some()
            {
                return Err(LmdbStateStoreError::InvalidLedgerState);
            }
            None
        };
        self.prune_superseded_state(&mut transaction, current, requested.checkpoint)?;
        self.put_state_checked(&mut transaction, requested.checkpoint, &encoded_state)?;
        if let Some(rows) = ledger_rows {
            self.sync_ledger_rows(&mut transaction, rows)?;
        }
        if let Some((state, update)) = authenticated_update {
            self.require_checkpoint_matches_authenticated_root(requested.checkpoint, state)?;
            self.authenticated_tree
                .apply_update(&mut transaction, &update)?;
            let encoded = encode_authenticated_state(state);
            self.metadata.put(
                &mut transaction,
                AUTHENTICATED_STATE_KEY,
                encoded.as_slice(),
            )?;
        }
        self.blocks.put(
            &mut transaction,
            block_key.as_slice(),
            encoded_block.as_slice(),
        )?;
        self.block_payloads
            .put(&mut transaction, block_key.as_slice(), verified.payload())?;
        let encoded_current = encode_current(requested.checkpoint);
        self.metadata
            .put(&mut transaction, CURRENT_KEY, encoded_current.as_slice())?;
        transaction.commit()?;
        Ok(CommitOutcome::Committed)
    }

    fn prepare_authenticated_transition(
        &self,
        transaction: &heed::RoTxn<'_>,
        current: FinalizedCheckpoint,
        next_height: u64,
        next_rows: &LedgerStateRows,
    ) -> Result<(StoredAuthenticatedState, jmt::storage::TreeUpdateBatch), LmdbStateStoreError>
    {
        let previous_rows = self.read_ledger_rows(transaction)?;
        let cursor = self.read_authenticated_state(transaction)?;
        if cursor.latest_height != current.height {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        let version = cursor
            .tree_version
            .checked_add(1)
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        let expected_height = cursor
            .base_height
            .checked_add(version)
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        if next_height != expected_height {
            return Err(LmdbStateStoreError::CursorMismatch);
        }
        let mutations = authenticated_state_delta(&previous_rows, next_rows)
            .map_err(|_| LmdbStateStoreError::InvalidLedgerState)?;
        let (root, update) =
            self.authenticated_tree
                .prepare_update(transaction, version, mutations)?;
        Ok((
            StoredAuthenticatedState {
                base_height: cursor.base_height,
                latest_height: next_height,
                tree_version: version,
                root: state_sync_core::StateRoot::new(*root.as_bytes()),
            },
            update,
        ))
    }

    fn replace_ledger_rows(
        &self,
        transaction: &mut heed::RwTxn<'_>,
        rows: &LedgerStateRows,
    ) -> Result<(), LmdbStateStoreError> {
        for database in [
            self.assets,
            self.balances,
            self.nonces,
            self.operation_sequence,
            self.account_statuses,
            self.contracts,
            self.contract_state,
        ] {
            database.clear(transaction)?;
        }
        write_rows(self.assets, transaction, &rows.assets)?;
        write_rows(self.balances, transaction, &rows.balances)?;
        write_rows(self.nonces, transaction, &rows.nonces)?;
        write_rows(
            self.operation_sequence,
            transaction,
            &rows.operation_sequence,
        )?;
        write_rows(self.account_statuses, transaction, &rows.account_statuses)?;
        write_rows(self.contracts, transaction, &rows.contracts)?;
        write_rows(self.contract_state, transaction, &rows.contract_state)
    }

    fn sync_ledger_rows(
        &self,
        transaction: &mut heed::RwTxn<'_>,
        rows: &LedgerStateRows,
    ) -> Result<(), LmdbStateStoreError> {
        let registry = self
            .metadata
            .get(transaction, REGISTRY_AUTHORITY_KEY)?
            .ok_or(LmdbStateStoreError::InvalidLedgerState)?;
        if registry != rows.registry_authority.as_bytes() {
            return Err(LmdbStateStoreError::InvalidLedgerState);
        }
        sync_rows(self.assets, transaction, &rows.assets)?;
        sync_rows(self.balances, transaction, &rows.balances)?;
        sync_rows(self.nonces, transaction, &rows.nonces)?;
        sync_rows(
            self.operation_sequence,
            transaction,
            &rows.operation_sequence,
        )?;
        sync_rows(self.account_statuses, transaction, &rows.account_statuses)?;
        sync_rows(self.contracts, transaction, &rows.contracts)?;
        sync_rows(self.contract_state, transaction, &rows.contract_state)
    }

    fn require_checkpoint_matches_authenticated_root(
        &self,
        checkpoint: FinalizedCheckpoint,
        authenticated: StoredAuthenticatedState,
    ) -> Result<(), LmdbStateStoreError> {
        if self.commitment_policy.scheme_at(checkpoint.height)
            == StateCommitmentScheme::AuthenticatedStateV1
            && checkpoint.state_root != authenticated.root
        {
            return Err(LmdbStateStoreError::InvalidLedgerState);
        }
        Ok(())
    }
}
