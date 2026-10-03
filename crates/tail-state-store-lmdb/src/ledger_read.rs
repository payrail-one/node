use ledger_core::LedgerSnapshot;
use ledger_runtime_core::{
    LedgerStateCodec, LedgerStateNamespace, LedgerStateProof, LedgerStateRows,
    authenticated_state_root,
};
use state_sync_core::FinalizedCheckpoint;

use crate::{
    LmdbStateStoreError, StoredAuthenticatedState, StoredLedgerProof, StoredLedgerState,
    codec::{
        decode_authenticated_state, decode_block, decode_commitment_policy, decode_current,
        decode_state,
    },
    rows::read_rows,
    store::{
        AUTHENTICATED_STATE_KEY, BASE_KEY, COMMITMENT_POLICY_KEY, CURRENT_KEY, LmdbTailStateStore,
        PROTOCOL_CONFIG_DIGEST_KEY, REGISTRY_AUTHORITY_KEY, validate_next,
    },
};

impl LmdbTailStateStore {
    /// Reads one account nonce from the current authenticated normalized state.
    ///
    /// This avoids rebuilding the complete ledger snapshot on the retail
    /// payment hot path. The returned value is accepted only when its JMT proof
    /// verifies against the current persisted authenticated root.
    ///
    /// # Errors
    ///
    /// Returns an error when network configuration, authenticated state, proof
    /// or nonce encoding is missing or corrupt.
    pub fn current_nonce(
        &self,
        account: ledger_core::AccountId,
    ) -> Result<ledger_core::Nonce, LmdbStateStoreError> {
        let authenticated = self.current_authenticated_state()?;
        let proof = self.prove_ledger_state(
            authenticated.latest_height,
            LedgerStateNamespace::Nonce,
            account.as_bytes(),
        )?;
        if !proof.verifies() {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        match proof.value() {
            None => Ok(0),
            Some(encoded) => {
                let nonce = u64::from_be_bytes(
                    encoded
                        .try_into()
                        .map_err(|_| LmdbStateStoreError::CorruptRecord)?,
                );
                if nonce == 0 {
                    Err(LmdbStateStoreError::CorruptRecord)
                } else {
                    Ok(nonce)
                }
            }
        }
    }

    /// Reads and validates normalized rows paired with the finalized cursor.
    ///
    /// # Errors
    ///
    /// Returns an error when rows are absent, corrupt or do not reproduce the
    /// finalized full-state root.
    pub fn current_ledger(&self) -> Result<StoredLedgerState, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        let checkpoint = self.read_current(&transaction)?;
        let snapshot = self.read_ledger_snapshot(&transaction)?;
        let state =
            LedgerStateCodec::encode(&snapshot).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
        self.require_network_config(&transaction)?;
        if self
            .commitment_policy
            .root_for_state(checkpoint.height, self.network, &state)
            .map_err(|_| LmdbStateStoreError::CorruptRecord)?
            != checkpoint.state_root
        {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        Ok(StoredLedgerState {
            checkpoint,
            snapshot,
        })
    }

    /// Reads and validates the canonical ledger snapshot at the recovery base.
    ///
    /// # Errors
    ///
    /// Returns an error when the base state is absent, malformed, for another
    /// network or inconsistent with the configured commitment policy.
    pub fn recovery_base_ledger(&self) -> Result<StoredLedgerState, LmdbStateStoreError> {
        let base = self.recovery_base()?;
        let snapshot = LedgerStateCodec::decode(self.network, &base.state)
            .map_err(|_| LmdbStateStoreError::CorruptRecord)?;
        self.verified_config
            .ok_or(LmdbStateStoreError::VerifiedNetworkConfigRequired)?;
        if self
            .commitment_policy
            .root_for_state(base.checkpoint.height, self.network, &base.state)
            .map_err(|_| LmdbStateStoreError::CorruptRecord)?
            != base.checkpoint.state_root
        {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        Ok(StoredLedgerState {
            checkpoint: base.checkpoint,
            snapshot,
        })
    }

    /// Reads the authenticated normalized-state cursor and verifies its root node.
    ///
    /// # Errors
    ///
    /// Returns an error when the ledger store is uninitialized or tree records
    /// are missing, inconsistent or corrupt.
    pub fn current_authenticated_state(
        &self,
    ) -> Result<StoredAuthenticatedState, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        self.require_network_config(&transaction)?;
        let state = self.read_authenticated_state(&transaction)?;
        let root = self
            .authenticated_tree
            .read_root(&transaction, state.tree_version)?;
        if root.as_bytes() != state.root.as_bytes() {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        Ok(state)
    }

    /// Reads a persisted membership or non-membership proof at a retained height.
    ///
    /// # Errors
    ///
    /// Returns an error for a height outside the retained tree, an invalid key,
    /// or missing/corrupt persistent nodes and values.
    pub fn prove_ledger_state(
        &self,
        height: u64,
        namespace: LedgerStateNamespace,
        key: &[u8],
    ) -> Result<StoredLedgerProof, LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        self.require_network_config(&transaction)?;
        let cursor = self.read_authenticated_state(&transaction)?;
        let version = height
            .checked_sub(cursor.base_height)
            .ok_or(LmdbStateStoreError::CursorMismatch)?;
        if version > cursor.tree_version {
            return Err(LmdbStateStoreError::CursorMismatch);
        }
        let root = self.authenticated_tree.read_root(&transaction, version)?;
        let (value, proof) = self.authenticated_tree.read_with_proof(
            &transaction,
            version,
            namespace.state_namespace(),
            key,
        )?;
        Ok(StoredLedgerProof {
            height,
            root: state_sync_core::StateRoot::new(*root.as_bytes()),
            proof: LedgerStateProof::from_authenticated_parts(
                namespace,
                key.to_vec(),
                value,
                proof,
            ),
        })
    }

    pub(crate) fn validate_history(&self) -> Result<(), LmdbStateStoreError> {
        let transaction = self.env.read_txn()?;
        let base = self.metadata.get(&transaction, BASE_KEY)?;
        let current = self.metadata.get(&transaction, CURRENT_KEY)?;
        let (Some(base), Some(current)) = (base, current) else {
            return if base.is_none() && current.is_none() {
                Ok(())
            } else {
                Err(LmdbStateStoreError::CorruptRecord)
            };
        };
        let base = decode_current(base)?;
        let current = decode_current(current)?;
        self.require_decodable_state(&transaction, base)?;
        let mut previous = base;
        while previous.height < current.height {
            let height = previous
                .height
                .checked_add(1)
                .ok_or(LmdbStateStoreError::CorruptRecord)?;
            let encoded = self
                .blocks
                .get(&transaction, height.to_be_bytes().as_slice())?
                .ok_or(LmdbStateStoreError::CorruptRecord)?;
            let record = decode_block(encoded)?;
            if record.network != self.network {
                return Err(LmdbStateStoreError::WrongNetwork);
            }
            validate_next(previous, &record).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
            let payload = self
                .block_payloads
                .get(&transaction, height.to_be_bytes().as_slice())?
                .ok_or(LmdbStateStoreError::CorruptRecord)?;
            if crate::codec::payload_hash(payload) != record.payload_hash {
                return Err(LmdbStateStoreError::CorruptRecord);
            }
            previous = record.checkpoint;
        }
        if previous != current {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        self.require_decodable_state(&transaction, current)?;
        if self
            .metadata
            .get(&transaction, REGISTRY_AUTHORITY_KEY)?
            .is_some()
        {
            self.validate_ledger_state(&transaction, current)?;
        }
        Ok(())
    }

    fn validate_ledger_state(
        &self,
        transaction: &heed::RoTxn<'_>,
        current: FinalizedCheckpoint,
    ) -> Result<(), LmdbStateStoreError> {
        let snapshot = self.read_ledger_snapshot(transaction)?;
        let canonical =
            LedgerStateCodec::encode(&snapshot).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
        self.require_network_config(transaction)?;
        if self
            .commitment_policy
            .root_for_state(current.height, self.network, &canonical)
            .map_err(|_| LmdbStateStoreError::CorruptRecord)?
            != current.state_root
        {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        let stored = self
            .states
            .get(transaction, current.state_root.as_bytes().as_slice())?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        if decode_state(stored)? != canonical {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        let authenticated = self.read_authenticated_state(transaction)?;
        if authenticated.latest_height != current.height
            || authenticated_state_root(&snapshot)
                .map_err(|_| LmdbStateStoreError::CorruptRecord)?
                != authenticated.root
        {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        let persisted_root = self
            .authenticated_tree
            .read_root(transaction, authenticated.tree_version)?;
        if persisted_root.as_bytes() != authenticated.root.as_bytes() {
            return Err(LmdbStateStoreError::CorruptRecord);
        }
        Ok(())
    }

    pub(crate) fn read_authenticated_state(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<StoredAuthenticatedState, LmdbStateStoreError> {
        let encoded = self
            .metadata
            .get(transaction, AUTHENTICATED_STATE_KEY)?
            .ok_or(LmdbStateStoreError::InvalidLedgerState)?;
        decode_authenticated_state(encoded)
    }

    pub(crate) fn require_authenticated_state(
        &self,
        transaction: &heed::RoTxn<'_>,
        height: u64,
    ) -> Result<(), LmdbStateStoreError> {
        let state = self.read_authenticated_state(transaction)?;
        let root = self
            .authenticated_tree
            .read_root(transaction, state.tree_version)?;
        if state.latest_height == height && root.as_bytes() == state.root.as_bytes() {
            Ok(())
        } else {
            Err(LmdbStateStoreError::ConflictingBlock)
        }
    }

    pub(crate) fn require_network_config(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<(), LmdbStateStoreError> {
        let config = self
            .verified_config
            .ok_or(LmdbStateStoreError::VerifiedNetworkConfigRequired)?;
        let digest = self
            .metadata
            .get(transaction, PROTOCOL_CONFIG_DIGEST_KEY)?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        if digest != config.protocol_digest().as_bytes() {
            return Err(LmdbStateStoreError::NetworkConfigMismatch);
        }
        let encoded = self
            .metadata
            .get(transaction, COMMITMENT_POLICY_KEY)?
            .ok_or(LmdbStateStoreError::CorruptRecord)?;
        if decode_commitment_policy(encoded)? == config.state_commitment_policy() {
            Ok(())
        } else {
            Err(LmdbStateStoreError::CommitmentPolicyMismatch)
        }
    }

    fn read_ledger_snapshot(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<LedgerSnapshot, LmdbStateStoreError> {
        self.read_ledger_rows(transaction)?
            .into_snapshot(self.network)
            .map_err(|_| LmdbStateStoreError::CorruptRecord)
    }

    pub(crate) fn read_ledger_rows(
        &self,
        transaction: &heed::RoTxn<'_>,
    ) -> Result<LedgerStateRows, LmdbStateStoreError> {
        let registry = self
            .metadata
            .get(transaction, REGISTRY_AUTHORITY_KEY)?
            .ok_or(LmdbStateStoreError::InvalidLedgerState)?;
        let registry_authority = ledger_core::AccountId::new(
            registry
                .try_into()
                .map_err(|_| LmdbStateStoreError::CorruptRecord)?,
        );
        Ok(LedgerStateRows {
            network: self.network,
            registry_authority,
            assets: read_rows(self.assets, transaction)?,
            balances: read_rows(self.balances, transaction)?,
            nonces: read_rows(self.nonces, transaction)?,
            operation_sequence: read_rows(self.operation_sequence, transaction)?,
            account_statuses: read_rows(self.account_statuses, transaction)?,
        })
    }
}

impl receipt_index_core::FinalizedLedgerHistory for LmdbTailStateStore {
    type Error = LmdbStateStoreError;

    fn recovery_base(&self) -> Result<receipt_index_core::FinalizedLedgerBase, Self::Error> {
        let stored = self.recovery_base_ledger()?;
        Ok(receipt_index_core::FinalizedLedgerBase {
            checkpoint: stored.checkpoint,
            snapshot: stored.snapshot,
        })
    }

    fn latest_checkpoint(&self) -> Result<FinalizedCheckpoint, Self::Error> {
        self.current_ledger().map(|stored| stored.checkpoint)
    }

    fn finalized_block(
        &self,
        height: u64,
    ) -> Result<receipt_index_core::ArchivedLedgerBlock, Self::Error> {
        let stored = LmdbTailStateStore::finalized_block(self, height)?;
        Ok(receipt_index_core::ArchivedLedgerBlock {
            previous: stored.previous,
            checkpoint: stored.checkpoint,
            payload: stored.payload,
        })
    }
}
