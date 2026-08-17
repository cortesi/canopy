//! Helpers for managing keyed child collections.

use std::{
    any::TypeId,
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
pub struct KeyedChildren<K, W> {
    /// Mapping from key to node ID.
    map: HashMap<K, TypedId<W>>,
    /// Ordered keys for child traversal.
    order: Vec<K>,
}

impl<K, W> Default for KeyedChildren<K, W> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
        }
    }
}

impl<K, W> KeyedChildren<K, W>
where
    K: Eq + Hash + Clone,
    W: Widget + 'static,
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

    /// Return the node ID for a key, if present.
    pub fn id_for(&self, key: &K) -> Option<TypedId<W>> {
        self.map.get(key).copied()
    }

    /// Return the node ID at a given index, if present.
    pub fn id_at(&self, index: usize) -> Option<TypedId<W>> {
        self.order.get(index).and_then(|key| self.id_for(key))
    }

    /// Iterate node IDs in the current order.
    pub fn iter_ids(&self) -> impl Iterator<Item = TypedId<W>> + '_ {
        self.order
            .iter()
            .filter_map(|key| self.map.get(key).copied())
    }

    /// Reconcile this collection against the desired key order.
    pub fn reconcile<I, C, U>(
        &mut self,
        ctx: &mut dyn Context,
        desired: I,
        mut create: C,
        mut update: U,
        remove: RemovePolicy,
    ) -> Result<Vec<TypedId<W>>>
    where
        I: IntoIterator<Item = K>,
        C: FnMut(&K) -> Result<W>,
        U: FnMut(&K, TypedId<W>, &mut dyn Context) -> Result<()>,
    {
        let desired: Vec<K> = desired.into_iter().collect();
        let mut seen = HashSet::with_capacity(desired.len());
        for key in &desired {
            if !seen.insert(key.clone()) {
                return Err(Error::Invalid("duplicate key in reconcile".into()));
            }
        }

        let parent = ctx.node_id();
        let children = ctx.children();
        let direct_children: HashSet<NodeId> = children.iter().copied().collect();
        let expected_type = TypeId::of::<W>();
        let mut planned_map = HashMap::with_capacity(self.map.len() + desired.len());
        for (key, typed_id) in &self.map {
            let node_id = NodeId::from(*typed_id);
            let Some(actual_type) = ctx.node_type_id(node_id) else {
                continue;
            };
            if actual_type != expected_type {
                return Err(Error::Invalid(
                    "keyed child points to a different widget type".into(),
                ));
            }
            let retained_hidden =
                matches!(remove, RemovePolicy::Hide) && ctx.parent_of(node_id).is_none();
            if direct_children.contains(&node_id) || retained_hidden {
                planned_map.insert(key.clone(), *typed_id);
            }
        }

        let mut candidates = Vec::new();
        for key in &desired {
            if !planned_map.contains_key(key) {
                candidates.push((key.clone(), create(key)?));
            }
        }

        let desired_set: HashSet<K> = desired.iter().cloned().collect();
        let removed: Vec<K> = children
            .iter()
            .filter_map(|node_id| {
                planned_map
                    .iter()
                    .find(|(key, mapped)| {
                        NodeId::from(**mapped) == *node_id && !desired_set.contains(*key)
                    })
                    .map(|(key, _)| key.clone())
            })
            .collect();
        let mut planned_map = Some(planned_map);
        let mut candidates = Some(candidates);
        let mut outcome = None;

        ctx.apply_tree_edit(&mut |ctx| {
            let mut working_map = planned_map
                .take()
                .ok_or_else(|| Error::Internal("reconcile map consumed".into()))?;
            for (key, widget) in candidates
                .take()
                .ok_or_else(|| Error::Internal("reconcile candidates consumed".into()))?
            {
                let typed_id = ctx.create_detached(widget)?;
                working_map.insert(key, typed_id);
            }

            let mut ordered = Vec::with_capacity(desired.len());
            for key in &desired {
                let typed_id = working_map
                    .get(key)
                    .copied()
                    .ok_or_else(|| Error::Internal("reconcile candidate missing".into()))?;
                let node_id = NodeId::from(typed_id);
                if ctx.node_type_id(node_id) != Some(expected_type) {
                    return Err(Error::Invalid(
                        "keyed child became stale during update".into(),
                    ));
                }
                update(key, typed_id, ctx)?;
                ordered.push(typed_id);
            }

            for key in &removed {
                let Some(typed_id) = working_map.get(key).copied() else {
                    continue;
                };
                let node_id = NodeId::from(typed_id);
                if ctx.node_type_id(node_id).is_none() {
                    working_map.remove(key);
                    continue;
                }
                match remove {
                    RemovePolicy::Detach => {
                        ctx.detach(node_id)?;
                        working_map.remove(key);
                    }
                    RemovePolicy::RemoveSubtree => {
                        ctx.remove_subtree(node_id)?;
                        working_map.remove(key);
                    }
                    RemovePolicy::Hide => {
                        ctx.set_hidden_of(node_id, true)?;
                    }
                }
            }

            for typed_id in &ordered {
                let node_id = NodeId::from(*typed_id);
                if ctx.node_type_id(node_id) != Some(expected_type) {
                    return Err(Error::Invalid(
                        "keyed child became stale before commit".into(),
                    ));
                }
                if matches!(remove, RemovePolicy::Hide) {
                    ctx.set_hidden_of(node_id, false)?;
                }
            }

            let ordered_nodes = ordered.iter().map(|id| NodeId::from(*id)).collect();
            ctx.set_children_of(parent, ordered_nodes)?;
            outcome = Some((working_map, ordered));
            Ok(())
        })?;

        let (map, ordered) =
            outcome.ok_or_else(|| Error::Internal("reconcile outcome missing".into()))?;
        self.map = map;
        self.order = desired;
        Ok(ordered)
    }
}
