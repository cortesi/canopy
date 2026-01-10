//! Helpers for managing keyed child collections.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::{
    Context, NodeId, TypedId, Widget,
    error::{Error, Result},
};

/// Policy for removing children that are no longer desired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovePolicy {
    /// Detach nodes from the tree but keep them alive.
    Detach,
    /// Remove nodes and their descendants from the arena.
    RemoveSubtree,
    /// Hide nodes and keep them available for reuse.
    Hide,
}

/// Ordered keyed child collection helper.
///
/// Stores a stable mapping from keys to node IDs plus a current order. Use
/// [`KeyedChildren::reconcile`] to create, update, and reorder children based on a desired key list.
#[derive(Debug)]
pub struct KeyedChildren<K> {
    /// Mapping from key to node ID.
    map: HashMap<K, NodeId>,
    /// Ordered keys for child traversal.
    order: Vec<K>,
}

impl<K> Default for KeyedChildren<K> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
        }
    }
}

impl<K> KeyedChildren<K>
where
    K: Eq + Hash + Clone,
{
    /// Construct an empty keyed collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return true if there are no ordered keys.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Return the number of ordered keys.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Return the ordered key slice.
    pub fn keys(&self) -> &[K] {
        &self.order
    }

    /// Return the key at a given index.
    pub fn key_at(&self, index: usize) -> Option<&K> {
        self.order.get(index)
    }

    /// Return the node ID for a key, if present.
    pub fn id_for(&self, key: &K) -> Option<NodeId> {
        self.map.get(key).copied()
    }

    /// Return the node ID at a given index, if present.
    pub fn id_at(&self, index: usize) -> Option<NodeId> {
        self.key_at(index).and_then(|key| self.id_for(key))
    }

    /// Iterate node IDs in the current order.
    pub fn iter_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.order
            .iter()
            .filter_map(|key| self.map.get(key).copied())
    }

    /// Reconcile this collection against the desired key order.
    pub fn reconcile<W, I, C, U>(
        &mut self,
        ctx: &mut dyn Context,
        desired: I,
        mut create: C,
        mut update: U,
        remove: RemovePolicy,
    ) -> Result<Vec<TypedId<W>>>
    where
        W: Widget + 'static,
        I: IntoIterator<Item = K>,
        C: FnMut(&K) -> W,
        U: FnMut(&K, TypedId<W>, &mut dyn Context) -> Result<()>,
    {
        let desired: Vec<K> = desired.into_iter().collect();
        let mut seen = HashSet::with_capacity(desired.len());
        for key in &desired {
            if !seen.insert(key.clone()) {
                return Err(Error::Invalid("duplicate key in reconcile".into()));
            }
        }

        let desired_set: HashSet<K> = desired.iter().cloned().collect();
        let mut removed = Vec::new();
        for key in self.map.keys() {
            if !desired_set.contains(key) {
                removed.push(key.clone());
            }
        }

        for key in removed {
            if let Some(id) = self.map.get(&key).copied() {
                match remove {
                    RemovePolicy::Detach => {
                        ctx.detach(id)?;
                        self.map.remove(&key);
                    }
                    RemovePolicy::RemoveSubtree => {
                        ctx.remove_subtree(id)?;
                        self.map.remove(&key);
                    }
                    RemovePolicy::Hide => {
                        ctx.set_hidden_of(id, true);
                    }
                }
            } else {
                self.map.remove(&key);
            }
        }

        let mut ordered = Vec::with_capacity(desired.len());
        for key in &desired {
            let id = if let Some(id) = self.map.get(key).copied() {
                if matches!(remove, RemovePolicy::Hide) {
                    ctx.set_hidden_of(id, false);
                }
                id
            } else {
                let id = ctx.add_child(create(key))?;
                let node_id = NodeId::from(id);
                self.map.insert(key.clone(), node_id);
                node_id
            };
            let typed_id = TypedId::new(id);
            update(key, typed_id, ctx)?;
            ordered.push(typed_id);
        }

        let ordered_nodes: Vec<NodeId> = ordered.iter().map(|id| NodeId::from(*id)).collect();
        ctx.set_children(ordered_nodes)?;

        self.order = desired;
        Ok(ordered)
    }
}
