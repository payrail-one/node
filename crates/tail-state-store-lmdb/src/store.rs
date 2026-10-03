use std::{fs, path::Path};

use heed::{Database, Env, EnvOpenOptions, types::Bytes};
use ledger_core::NetworkId;
use ledger_runtime_core::StateCommitmentPolicy;
use network_config_core::VerifiedNetworkConfig;
use state_sync_core::{FinalizedCheckpoint, SyncCompletion};
use tail_sync_core::VerifiedTailBlock;

use crate::{
    CommitOutcome, LmdbStateStoreError, LmdbStoreOptions, StoredFinalizedBlock, StoredTailState,
    authenticated_tree::AuthenticatedTreeDatabases,
    codec::{
        BlockRecord, decode_commitment_policy, decode_current, decode_state, encode_current,
        encode_state,
    },
    types::MINIMUM_MAP_SIZE,
};

const NETWORK_KEY: &[u8] = b"network";
pub(crate) const BASE_KEY: &[u8] = b"base";
pub(crate) const CURRENT_KEY: &[u8] = b"current";
pub(crate) const REGISTRY_AUTHORITY_KEY: &[u8] = b"ledger-registry-authority";
pub(crate) const AUTHENTICATED_STATE_KEY: &[u8] = b"authenticated-state";
pub(crate) const COMMITMENT_POLICY_KEY: &[u8] = b"state-commitment-policy";
pub(crate) const PROTOCOL_CONFIG_DIGEST_KEY: &[u8] = b"protocol-config-digest";
const MAX_DATABASES: u32 = 16;

#[derive(Debug)]
pub struct LmdbTailStateStore {
    pub(crate) env: Env,
    pub(crate) metadata: Database<Bytes, Bytes>,
    pub(crate) blocks: Database<Bytes, Bytes>,
    pub(crate) block_payloads: Database<Bytes, Bytes>,
    pub(crate) states: Database<Bytes, Bytes>,
    pub(crate) assets: Database<Bytes, Bytes>,
    pub(crate) balances: Database<Bytes, Bytes>,
    pub(crate) nonces: Database<Bytes, Bytes>,
    pub(crate) operation_sequence: Database<Bytes, Bytes>,
    pub(crate) account_statuses: Database<Bytes, Bytes>,
    pub(crate) authenticated_tree: AuthenticatedTreeDatabases,
    pub(crate) network: NetworkId,
    pub(crate) commitment_policy: StateCommitmentPolicy,
    pub(crate) verified_config: Option<VerifiedNetworkConfig>,
}

impl LmdbTailStateStore {
    #[must_use]
    pub const fn network(&self) -> NetworkId {
        self.network
    }

    /// Opens a local LMDB environment without unsafe durability flags.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths, invalid options, wrong-network data,
    /// corrupt records or LMDB failures.
    pub fn open(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: LmdbStoreOptions,
    ) -> Result<Self, LmdbStateStoreError> {
        Self::open_internal(path, network, options, None)
    }

    /// Opens a ledger store bound to a previously signature-verified network config.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe options, corrupt records, or a config digest
    /// that differs from the one persisted for an existing ledger.
    pub fn open_ledger(
        path: impl AsRef<Path>,
        options: LmdbStoreOptions,
        verified_config: VerifiedNetworkConfig,
    ) -> Result<Self, LmdbStateStoreError> {
        Self::open_internal(
            path,
            verified_config.network(),
            options,
            Some(verified_config),
        )
    }

    fn open_internal(
        path: impl AsRef<Path>,
        network: NetworkId,
        options: LmdbStoreOptions,
        verified_config: Option<VerifiedNetworkConfig>,
    ) -> Result<Self, LmdbStateStoreError> {
        if options.map_size < MINIMUM_MAP_SIZE {
            return Err(LmdbStateStoreError::MapSizeTooSmall);
        }
        let requested = path.as_ref();
        fs::create_dir_all(requested)?;
        let metadata = fs::symlink_metadata(requested)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(LmdbStateStoreError::UnsafePath);
        }
        let path = fs::canonicalize(requested)?;
        let env = open_environment(&path, options)?;
        let mut transaction = env.write_txn()?;
        let metadata = env.create_database(&mut transaction, Some("metadata"))?;
        let blocks = env.create_database(&mut transaction, Some("blocks"))?;
        let block_payloads = env.create_database(&mut transaction, Some("block_payloads"))?;
        let states = env.create_database(&mut transaction, Some("states"))?;
        let assets = env.create_database(&mut transaction, Some("ledger_assets"))?;
        let balances = env.create_database(&mut transaction, Some("ledger_balances"))?;
        let nonces = env.create_database(&mut transaction, Some("ledger_nonces"))?;
        let operation_sequence =
            env.create_database(&mut transaction, Some("ledger_operation_sequence"))?;
        let account_statuses =
            env.create_database(&mut transaction, Some("ledger_account_statuses"))?;
        let authenticated_tree = AuthenticatedTreeDatabases::create(&env, &mut transaction)?;
        match metadata.get(&transaction, NETWORK_KEY)? {
            Some(stored) if stored != network.as_bytes() => {
                return Err(LmdbStateStoreError::WrongNetwork);
            }
            Some(_) => {}
            None => metadata.put(&mut transaction, NETWORK_KEY, network.as_bytes().as_slice())?,
        }
        reconcile_network_config(&metadata, &transaction, verified_config)?;
        transaction.commit()?;
        let commitment_policy = verified_config
            .map(VerifiedNetworkConfig::state_commitment_policy)
            .unwrap_or_default();
        let store = Self {
            env,
            metadata,
            blocks,
            block_payloads,
            states,
            assets,
            balances,
            nonces,
            operation_sequence,
            account_statuses,
            authenticated_tree,
            network,
            commitment_policy,
            verified_config,
        };
        store.validate_history()?;
        Ok(store)
    }

    #[must_use]
    pub const fn commitment_policy(&self) -> StateCommitmentPolicy {
        self.commitment_policy
    }

    /// Initializes the authoritative cursor and state restored from a verified snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the store is initialized, or if encoding/LMDB fails.
    pub fn initialize_from_snapshot(
        &self,
        completion: SyncCompletion,
        state: &[u8],
    ) -> Result<(), LmdbStateStoreError> {
        let mut transaction = self.env.write_txn()?;
        if self.metadata.get(&transaction, BASE_KEY)?.is_some()
            || self.metadata.get(&transaction, CURRENT_KEY)?.is_some()
        {
            return Err(LmdbStateStoreError::AlreadyInitialized);
        }
        let checkpoint = completion.checkpoint;
        let encoded_checkpoint = encode_current(checkpoint);
        let encoded_state = encode_state(state)?;
        self.put_state_checked(&mut transaction, checkpoint, &encoded_state)?;
        self.metadata
            .put(&mut transaction, BASE_KEY, encoded_checkpoint.as_slice())?;
        self.metadata
            .put(&mut transaction, CURRENT_KEY, encoded_checkpoint.as_slice())?;
        transaction.commit()?;
        Ok(())
    }

    /// Reads the atomically paired finalized cursor and canonical state image.
    ///
    /// # Errors
    ///
    /// Returns an error when uninitialized or when either record is corrupt.
    pub fn current(&self) -> Result<StoredTailState, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        let checkpoint = self.read_current(&transaction)?;
        let encoded_state = self
            .states
            .get(&transaction, checkpoint.state_root.as_bytes().as_slice())?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        Ok(StoredTailState {
            checkpoint,
            state: decode_state(encoded_state)?,
        })
    }

    /// Reads the verified recovery-base cursor and canonical state image.
    ///
    /// # Errors
    ///
    /// Returns an error when the store is uninitialized or the base image is
    /// missing or corrupt.
    pub fn recovery_base(&self) -> Result<StoredTailState, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        let encoded_checkpoint = self
            .metadata
            .get(&transaction, BASE_KEY)?
            .ok_or(LmdbStateStoreError::NotInitialized)?;
        let checkpoint = decode_current(encoded_checkpoint)?;
        let encoded_state = self
            .states
            .get(&transaction, checkpoint.state_root.as_bytes().as_slice())?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        Ok(StoredTailState {
            checkpoint,
            state: decode_state(encoded_state)?,
        })
    }

    /// Returns the number of retained complete canonical state images.
    ///
    /// The finalized store retains the recovery base and current image; block
    /// records and authenticated historical proofs are stored separately.
    ///
    /// # Errors
    ///
    /// Returns an error when LMDB cannot read the state database.
    pub fn retained_state_count(&self) -> Result<u64, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        self.states.len(&transaction).map_err(Into::into)
    }

    /// Returns the number of complete retained post-base block payloads.
    ///
    /// # Errors
    ///
    /// Returns an error when LMDB cannot read the payload database.
    pub fn retained_payload_count(&self) -> Result<u64, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        self.block_payloads.len(&transaction).map_err(Into::into)
    }

    /// Reads the canonical payload retained for a finalized post-base height.
    ///
    /// # Errors
    ///
    /// Returns an error when the height is not retained or the payload record
    /// does not match its immutable block audit hash.
    pub fn finalized_payload(&self, height: u64) -> Result<Vec<u8>, LmdbStateStoreError> {
        self.finalized_block(height).map(|block| block.payload)
    }

    /// Reads one complete hash-verified finalized post-base block.
    ///
    /// # Errors
    ///
    /// Returns an error when the height is not retained or the record/payload
    /// is corrupt.
    pub fn finalized_block(
        &self,
        height: u64,
    ) -> Result<StoredFinalizedBlock, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        let key = height.to_be_bytes();
        let encoded_block = self
            .blocks
            .get(&transaction, key.as_slice())?
            .ok_or(LmdbStateStoreError::BlockNotFound)?;
        let block = crate::codec::decode_block(encoded_block)?;
        let payload = self
            .block_payloads
            .get(&transaction, key.as_slice())?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        if crate::codec::payload_hash(payload) != block.payload_hash {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        Ok(StoredFinalizedBlock {
            previous: block.previous,
            checkpoint: block.checkpoint,
            payload: payload.to_vec(),
        })
    }

    /// Atomically writes the prepared state, block audit record and finalized cursor.
    ///
    /// # Errors
    ///
    /// Returns an error for wrong-network, stale, conflicting or corrupt data,
    /// or when the LMDB transaction cannot commit durably.
    pub fn commit_verified(
        &self,
        verified: &VerifiedTailBlock,
    ) -> Result<CommitOutcome, LmdbStateStoreError> {
        self.commit_verified_internal(verified, None)
    }

    pub(crate) fn read_current(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<FinalizedCheckpoint, LmdbStateStoreError> {
        let encoded = self
            .metadata
            .get(transaction, CURRENT_KEY)?
            .ok_or(LmdbStateStoreError::NotInitialized)?;
        decode_current(encoded)
    }

    pub(crate) fn require_decodable_state(
        &self,
        transaction: &heed::RoTxn<'_>,
        checkpoint: FinalizedCheckpoint,
    ) -> Result<(), LmdbStateStoreError> {
        let encoded = self
            .states
            .get(transaction, checkpoint.state_root.as_bytes().as_slice())?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        decode_state(encoded).map(|_| ())
    }

    pub(crate) fn require_same_state(
        &self,
        transaction: &heed::RoTxn<'_>,
        checkpoint: FinalizedCheckpoint,
        encoded_state: &[u8],
    ) -> Result<(), LmdbStateStoreError> {
        match self
            .states
            .get(transaction, checkpoint.state_root.as_bytes().as_slice())?
        {
            Some(existing) if existing == encoded_state => Ok(()),
            _ => Err(LmdbStateStoreError::ConflictingBlock),
        }
    }

    pub(crate) fn put_state_checked(
        &self,
        transaction: &mut heed::RwTxn<'_>,
        checkpoint: FinalizedCheckpoint,
        encoded_state: &[u8],
    ) -> Result<(), LmdbStateStoreError> {
        let key = checkpoint.state_root.as_bytes().as_slice();
        match self.states.get(transaction, key)? {
            Some(existing) if existing == encoded_state => Ok(()),
            Some(_) => Err(LmdbStateStoreError::ConflictingBlock),
            None => {
                self.states.put(transaction, key, encoded_state)?;
                Ok(())
            }
        }
    }

    pub(crate) fn prune_superseded_state(
        &self,
        transaction: &mut heed::RwTxn<'_>,
        current: FinalizedCheckpoint,
        next: FinalizedCheckpoint,
    ) -> Result<(), LmdbStateStoreError> {
        let base = self
            .metadata
            .get(transaction, BASE_KEY)?
            .ok_or(LmdbStateStoreError::CorruptRecord)
            .and_then(decode_current)?;
        if current.state_root != base.state_root && current.state_root != next.state_root {
            let removed = self
                .states
                .delete(transaction, current.state_root.as_bytes().as_slice())?;
            if !removed {
                return Err(LmdbStateStoreError::CorruptRecord);
            }
        }
        Ok(())
    }
}

fn reconcile_network_config(
    metadata: &Database<Bytes, Bytes>,
    transaction: &heed::RoTxn<'_>,
    requested: Option<VerifiedNetworkConfig>,
) -> Result<(), LmdbStateStoreError> {
    let stored_digest = metadata.get(transaction, PROTOCOL_CONFIG_DIGEST_KEY)?;
    let ledger_exists = metadata.get(transaction, REGISTRY_AUTHORITY_KEY)?.is_some();
    match (requested, stored_digest, ledger_exists) {
        (Some(config), Some(stored), _) if stored == config.protocol_digest().as_bytes() => {
            let encoded_policy = metadata
                .get(transaction, COMMITMENT_POLICY_KEY)?
                .ok_or(LmdbStateStoreError::CorruptRecord)?;
            if decode_commitment_policy(encoded_policy)? == config.state_commitment_policy() {
                Ok(())
            } else {
                Err(LmdbStateStoreError::CommitmentPolicyMismatch)
            }
        }
        (Some(_), Some(_), _) => Err(LmdbStateStoreError::NetworkConfigMismatch),
        (Some(_) | None, None, false) => Ok(()),
        (Some(_), None, true) | (None, _, true) | (None, Some(_), false) => {
            Err(LmdbStateStoreError::VerifiedNetworkConfigRequired)
        }
    }
}

pub(crate) fn validate_next(
    current: FinalizedCheckpoint,
    requested: &BlockRecord,
) -> Result<(), LmdbStateStoreError> {
    let expected_height = current
        .height
        .checked_add(1)
        .ok_or(LmdbStateStoreError::CursorMismatch)?;
    if requested.previous != current
        || requested.checkpoint.height != expected_height
        || requested.checkpoint.block_hash == current.block_hash
    {
        return Err(LmdbStateStoreError::CursorMismatch);
    }
    Ok(())
}

#[allow(unsafe_code)]
fn open_environment(path: &Path, options: LmdbStoreOptions) -> Result<Env, LmdbStateStoreError> {
    let mut builder = EnvOpenOptions::new();
    builder
        .map_size(options.map_size)
        .max_dbs(MAX_DATABASES)
        .max_readers(options.max_readers);
    // SAFETY: the canonical directory is application-owned and never modified
    // outside LMDB while this environment is live. We keep LMDB locking enabled,
    // use no NO_SYNC/NO_META_SYNC flags, and never expose mapped references.
    unsafe { builder.open(path) }.map_err(Into::into)
}
