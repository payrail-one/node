use std::collections::BTreeMap;

use anyhow::Result;
use jmt::{
    KeyHash, OwnedValue, Version,
    storage::{LeafNode, Node, NodeBatch, NodeKey, TreeReader, TreeUpdateBatch},
};

use crate::AuthenticatedStateError;

#[derive(Clone, Debug, Default)]
pub(crate) struct MemoryTreeStore {
    nodes: BTreeMap<NodeKey, Node>,
    values: BTreeMap<KeyHash, Vec<(Version, Option<OwnedValue>)>>,
    live_leaves: BTreeMap<KeyHash, NodeKey>,
}

impl MemoryTreeStore {
    pub(crate) fn apply(
        &mut self,
        update: &TreeUpdateBatch,
    ) -> Result<(), AuthenticatedStateError> {
        self.validate_update(update)?;
        self.apply_node_batch(&update.node_batch);
        self.apply_live_leaves(update);
        Ok(())
    }

    fn validate_update(&self, update: &TreeUpdateBatch) -> Result<(), AuthenticatedStateError> {
        if update
            .node_batch
            .nodes()
            .keys()
            .any(|key| self.nodes.contains_key(key))
        {
            return Err(AuthenticatedStateError::StoreConflict);
        }
        for (version, key) in update.node_batch.values().keys() {
            if self
                .values
                .get(key)
                .and_then(|history| history.last())
                .is_some_and(|(last_version, _)| last_version >= version)
            {
                return Err(AuthenticatedStateError::StoreConflict);
            }
        }
        for stale in &update.stale_node_index_batch {
            let exists = self.nodes.contains_key(&stale.node_key)
                || update.node_batch.nodes().contains_key(&stale.node_key);
            if !exists {
                return Err(AuthenticatedStateError::StoreConflict);
            }
        }
        Ok(())
    }

    fn apply_live_leaves(&mut self, update: &TreeUpdateBatch) {
        for stale in &update.stale_node_index_batch {
            if let Some(Node::Leaf(leaf)) = self.nodes.get(&stale.node_key)
                && self.live_leaves.get(&leaf.key_hash()) == Some(&stale.node_key)
            {
                self.live_leaves.remove(&leaf.key_hash());
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
            if let Node::Leaf(leaf) = node {
                self.live_leaves.insert(leaf.key_hash(), node_key.clone());
            }
        }
    }

    fn apply_node_batch(&mut self, batch: &NodeBatch) {
        for (key, node) in batch.nodes() {
            self.nodes.insert(key.clone(), node.clone());
        }
        for ((version, key), value) in batch.values() {
            let history = self.values.entry(*key).or_default();
            history.push((*version, value.clone()));
        }
    }

    pub(crate) fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl TreeReader for MemoryTreeStore {
    fn get_node_option(&self, node_key: &NodeKey) -> Result<Option<Node>> {
        Ok(self.nodes.get(node_key).cloned())
    }

    fn get_value_option(
        &self,
        max_version: Version,
        key_hash: KeyHash,
    ) -> Result<Option<OwnedValue>> {
        Ok(self.values.get(&key_hash).and_then(|history| {
            history
                .iter()
                .rev()
                .find(|(version, _)| *version <= max_version)
                .and_then(|(_, value)| value.clone())
        }))
    }

    fn get_rightmost_leaf(&self) -> Result<Option<(NodeKey, LeafNode)>> {
        let Some((key_hash, node_key)) = self.live_leaves.last_key_value() else {
            return Ok(None);
        };
        match self.nodes.get(node_key) {
            Some(Node::Leaf(leaf)) if leaf.key_hash() == *key_hash => {
                Ok(Some((node_key.clone(), leaf.clone())))
            }
            Some(Node::Leaf(_) | Node::Internal(_) | Node::Null) | None => {
                Err(anyhow::anyhow!("invalid live leaf index"))
            }
        }
    }
}

pub(crate) fn backend_error() -> AuthenticatedStateError {
    AuthenticatedStateError::BackendFailure
}

#[cfg(test)]
mod tests {
    use jmt::storage::TreeReader;

    use super::MemoryTreeStore;
    use crate::{StateMutation, StateNamespace, backend, state_key_hash};

    #[test]
    fn rightmost_leaf_index_excludes_deleted_historical_leaf() {
        let namespace = StateNamespace::new(1);
        let mut keys = [b"alice".to_vec(), b"bob".to_vec()];
        keys.sort_by_key(|key| state_key_hash(namespace.value(), key));
        let mut store = MemoryTreeStore::default();
        let (_, initial) = backend::prepare_update(
            &store,
            0,
            keys.iter()
                .map(|key| StateMutation::set(namespace, key.clone(), b"value".to_vec()))
                .collect(),
        )
        .unwrap();
        store.apply(&initial).unwrap();
        assert_eq!(
            store.get_rightmost_leaf().unwrap().unwrap().1.key_hash(),
            state_key_hash(namespace.value(), &keys[1])
        );

        let (_, deletion) = backend::prepare_update(
            &store,
            1,
            vec![StateMutation::delete(namespace, keys[1].clone())],
        )
        .unwrap();
        store.apply(&deletion).unwrap();
        assert_eq!(
            store.get_rightmost_leaf().unwrap().unwrap().1.key_hash(),
            state_key_hash(namespace.value(), &keys[0])
        );
    }
}
