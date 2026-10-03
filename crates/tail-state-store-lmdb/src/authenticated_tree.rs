use anyhow::{Result as AnyResult, anyhow};
use authenticated_state_core::{
    AuthenticatedStateError, AuthenticatedStateRoot, StateMutation, StateNamespace, StateProof,
    backend,
};
use borsh::{BorshDeserialize, to_vec};
use heed::{Database, Env, RoTxn, RwTxn, types::Bytes};
use jmt::{
    KeyHash, OwnedValue, Version,
    storage::{LeafNode, Node, NodeKey, TreeReader, TreeUpdateBatch},
};

use crate::LmdbStateStoreError;

const MAX_NODE_KEY_BYTES: usize = 128;
const MAX_NODE_BYTES: usize = 4 * 1024;
const VALUE_KEY_BYTES: usize = 40;
const MAX_VALUE_BYTES: usize = 512;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AuthenticatedTreeDatabases {
    nodes: Database<Bytes, Bytes>,
    values: Database<Bytes, Bytes>,
    stale_nodes: Database<Bytes, Bytes>,
    live_leaves: Database<Bytes, Bytes>,
}

impl AuthenticatedTreeDatabases {
    pub(crate) fn create(
        env: &Env,
        transaction: &mut RwTxn<'_>,
    ) -> Result<Self, LmdbStateStoreError> {
        Ok(Self {
            nodes: env.create_database(transaction, Some("authenticated_state_nodes"))?,
            values: env.create_database(transaction, Some("authenticated_state_values"))?,
            stale_nodes: env
                .create_database(transaction, Some("authenticated_state_stale_nodes"))?,
            live_leaves: env
                .create_database(transaction, Some("authenticated_state_live_leaves"))?,
        })
    }

    pub(crate) fn prepare_update(
        &self,
        transaction: &RoTxn<'_>,
        version: Version,
        mutations: Vec<StateMutation>,
    ) -> Result<(AuthenticatedStateRoot, TreeUpdateBatch), LmdbStateStoreError> {
        backend::prepare_update(&LmdbTreeReader::new(*self, transaction), version, mutations)
            .map_err(map_state_error)
    }

    pub(crate) fn read_root(
        &self,
        transaction: &RoTxn<'_>,
        version: Version,
    ) -> Result<AuthenticatedStateRoot, LmdbStateStoreError> {
        backend::read_root(&LmdbTreeReader::new(*self, transaction), version)
            .map_err(map_state_error)
    }

    pub(crate) fn read_with_proof(
        &self,
        transaction: &RoTxn<'_>,
        version: Version,
        namespace: StateNamespace,
        key: &[u8],
    ) -> Result<(Option<Vec<u8>>, StateProof), LmdbStateStoreError> {
        backend::read_with_proof(
            &LmdbTreeReader::new(*self, transaction),
            version,
            namespace,
            key,
        )
        .map_err(map_state_error)
    }

    pub(crate) fn apply_update(
        &self,
        transaction: &mut RwTxn<'_>,
        update: &TreeUpdateBatch,
    ) -> Result<(), LmdbStateStoreError> {
        for (node_key, node) in update.node_batch.nodes() {
            let key = encode_node_key(node_key)?;
            let value = encode_node(node)?;
            if self.nodes.get(transaction, &key)?.is_some() {
                return Err(LmdbStateStoreError::TreeConflict);
            }
            self.nodes.put(transaction, &key, &value)?;
        }
        for ((version, key_hash), value) in update.node_batch.values() {
            let key = value_key(*key_hash, *version);
            let value = encode_value(value.as_deref())?;
            if self.values.get(transaction, &key)?.is_some() {
                return Err(LmdbStateStoreError::TreeConflict);
            }
            self.values.put(transaction, &key, &value)?;
        }
        self.apply_stale_nodes(transaction, update)?;
        self.apply_live_leaves(transaction, update)
    }

    fn apply_stale_nodes(
        &self,
        transaction: &mut RwTxn<'_>,
        update: &TreeUpdateBatch,
    ) -> Result<(), LmdbStateStoreError> {
        for stale in &update.stale_node_index_batch {
            let node_key = encode_node_key(&stale.node_key)?;
            let mut key = Vec::with_capacity(8 + node_key.len());
            key.extend_from_slice(&stale.stale_since_version.to_be_bytes());
            key.extend_from_slice(&node_key);
            if self.stale_nodes.get(transaction, &key)?.is_some() {
                return Err(LmdbStateStoreError::TreeConflict);
            }
            self.stale_nodes.put(transaction, &key, &[])?;
        }
        Ok(())
    }

    fn apply_live_leaves(
        &self,
        transaction: &mut RwTxn<'_>,
        update: &TreeUpdateBatch,
    ) -> Result<(), LmdbStateStoreError> {
        for stale in &update.stale_node_index_batch {
            let encoded_key = encode_node_key(&stale.node_key)?;
            let node = read_node_from_batch_or_database(
                self.nodes,
                transaction,
                &update.node_batch,
                &stale.node_key,
                &encoded_key,
            )?;
            let Node::Leaf(leaf) = node else {
                continue;
            };
            let hash = leaf.key_hash().0;
            if self.live_leaves.get(transaction, &hash)? == Some(encoded_key.as_slice()) {
                self.live_leaves.delete(transaction, &hash)?;
            }
        }
        for (node_key, node) in update.node_batch.nodes() {
            if update
                .stale_node_index_batch
                .iter()
                .any(|stale| stale.node_key == *node_key)
            {
                continue;
            }
            let Node::Leaf(leaf) = node else {
                continue;
            };
            let encoded_key = encode_node_key(node_key)?;
            self.live_leaves
                .put(transaction, &leaf.key_hash().0, &encoded_key)?;
        }
        Ok(())
    }
}

struct LmdbTreeReader<'txn> {
    databases: AuthenticatedTreeDatabases,
    transaction: &'txn RoTxn<'txn>,
}

impl<'txn> LmdbTreeReader<'txn> {
    const fn new(databases: AuthenticatedTreeDatabases, transaction: &'txn RoTxn<'txn>) -> Self {
        Self {
            databases,
            transaction,
        }
    }
}

impl TreeReader for LmdbTreeReader<'_> {
    fn get_node_option(&self, node_key: &NodeKey) -> AnyResult<Option<Node>> {
        let key = encode_node_key(node_key).map_err(|_| anyhow!("invalid node key"))?;
        self.databases
            .nodes
            .get(self.transaction, &key)
            .map_err(|_| anyhow!("node database read failed"))?
            .map(decode_node)
            .transpose()
            .map_err(|_| anyhow!("invalid node record"))
    }

    fn get_value_option(
        &self,
        max_version: Version,
        key_hash: KeyHash,
    ) -> AnyResult<Option<OwnedValue>> {
        let iterator = self
            .databases
            .values
            .rev_prefix_iter(self.transaction, &key_hash.0)
            .map_err(|_| anyhow!("value database read failed"))?;
        for result in iterator {
            let (key, value) = result.map_err(|_| anyhow!("value iteration failed"))?;
            let version = decode_value_version(key).map_err(|_| anyhow!("invalid value key"))?;
            if version <= max_version {
                return decode_value(value).map_err(|_| anyhow!("invalid value record"));
            }
        }
        Ok(None)
    }

    fn get_rightmost_leaf(&self) -> AnyResult<Option<(NodeKey, LeafNode)>> {
        let Some((hash, encoded_key)) = self
            .databases
            .live_leaves
            .last(self.transaction)
            .map_err(|_| anyhow!("leaf index read failed"))?
        else {
            return Ok(None);
        };
        if hash.len() != 32 {
            return Err(anyhow!("invalid leaf index key"));
        }
        let node_key = decode_node_key(encoded_key).map_err(|_| anyhow!("invalid leaf key"))?;
        let node = self
            .get_node_option(&node_key)?
            .ok_or_else(|| anyhow!("missing indexed leaf"))?;
        match node {
            Node::Leaf(leaf) if leaf.key_hash().0.as_slice() == hash => Ok(Some((node_key, leaf))),
            Node::Leaf(_) | Node::Internal(_) | Node::Null => Err(anyhow!("invalid leaf index")),
        }
    }
}

fn read_node_from_batch_or_database(
    database: Database<Bytes, Bytes>,
    transaction: &RoTxn<'_>,
    batch: &jmt::storage::NodeBatch,
    node_key: &NodeKey,
    encoded_key: &[u8],
) -> Result<Node, LmdbStateStoreError> {
    if let Some(node) = batch.get_node(node_key) {
        return Ok(node.clone());
    }
    let encoded = database
        .get(transaction, encoded_key)?
        .ok_or(LmdbStateStoreError::CorruptRecord)?;
    decode_node(encoded)
}

fn encode_node_key(node_key: &NodeKey) -> Result<Vec<u8>, LmdbStateStoreError> {
    let encoded = to_vec(node_key).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
    if encoded.is_empty() || encoded.len() > MAX_NODE_KEY_BYTES {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Ok(encoded)
}

fn decode_node_key(encoded: &[u8]) -> Result<NodeKey, LmdbStateStoreError> {
    if encoded.is_empty() || encoded.len() > MAX_NODE_KEY_BYTES {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    NodeKey::try_from_slice(encoded).map_err(|_| LmdbStateStoreError::CorruptRecord)
}

fn encode_node(node: &Node) -> Result<Vec<u8>, LmdbStateStoreError> {
    let encoded = to_vec(node).map_err(|_| LmdbStateStoreError::CorruptRecord)?;
    if encoded.is_empty() || encoded.len() > MAX_NODE_BYTES {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Ok(encoded)
}

fn decode_node(encoded: &[u8]) -> Result<Node, LmdbStateStoreError> {
    if encoded.is_empty() || encoded.len() > MAX_NODE_BYTES {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Node::try_from_slice(encoded).map_err(|_| LmdbStateStoreError::CorruptRecord)
}

fn value_key(key_hash: KeyHash, version: Version) -> [u8; VALUE_KEY_BYTES] {
    let mut key = [0_u8; VALUE_KEY_BYTES];
    key[..32].copy_from_slice(&key_hash.0);
    key[32..].copy_from_slice(&version.to_be_bytes());
    key
}

fn decode_value_version(key: &[u8]) -> Result<Version, LmdbStateStoreError> {
    if key.len() != VALUE_KEY_BYTES {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    Ok(u64::from_be_bytes(
        key[32..]
            .try_into()
            .map_err(|_| LmdbStateStoreError::CorruptRecord)?,
    ))
}

fn encode_value(value: Option<&[u8]>) -> Result<Vec<u8>, LmdbStateStoreError> {
    if value.is_some_and(|bytes| bytes.len() > MAX_VALUE_BYTES) {
        return Err(LmdbStateStoreError::CorruptRecord);
    }
    let mut encoded = Vec::with_capacity(value.map_or(1, |bytes| bytes.len() + 1));
    encoded.push(u8::from(value.is_some()));
    if let Some(bytes) = value {
        encoded.extend_from_slice(bytes);
    }
    Ok(encoded)
}

fn decode_value(encoded: &[u8]) -> Result<Option<OwnedValue>, LmdbStateStoreError> {
    match encoded.split_first() {
        Some((0, [])) => Ok(None),
        Some((1, value)) if value.len() <= MAX_VALUE_BYTES => Ok(Some(value.to_vec())),
        _ => Err(LmdbStateStoreError::CorruptRecord),
    }
}

const fn map_state_error(error: AuthenticatedStateError) -> LmdbStateStoreError {
    match error {
        AuthenticatedStateError::BackendFailure | AuthenticatedStateError::StoreConflict => {
            LmdbStateStoreError::CorruptRecord
        }
        AuthenticatedStateError::InvalidVersion
        | AuthenticatedStateError::TooManyChanges
        | AuthenticatedStateError::EmptyKey
        | AuthenticatedStateError::KeyTooLarge
        | AuthenticatedStateError::ValueTooLarge
        | AuthenticatedStateError::DuplicateKey => LmdbStateStoreError::InvalidLedgerState,
    }
}
