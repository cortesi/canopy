//! The keyed child reconciler behind [`List`](crate::List).

use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    hash::Hash,
};

use canopy::{
    Context, ContextExt, NodeId, TypedId, Widget,
    error::{Error, Result},
};

/// Ordered keyed child collection helper.
///
/// Stores a stable mapping from keys to node IDs plus a current order. Use
/// [`KeyedChildren::reconcile`] to create, update, and reorder children based
/// on a desired key list. The collection owns all children of its context node;
/// unmanaged children are rejected before callbacks run. Place persistent
/// headers and footers outside a dedicated collection container.
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
    ///
    /// Errors restore structure and leave this collection unchanged. Mutations
    /// performed by `create` and `update` have the rollback limits documented
    /// by [`Context::edit_structure`], including retained widget state and
    /// external effects.
    pub fn reconcile<I, C, U>(
        &mut self,
        ctx: &mut dyn Context,
        desired: I,
        mut create: C,
        mut update: U,
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
        let children = ctx.children_of(ctx.node_id());
        let direct_children: HashSet<NodeId> = children.iter().copied().collect();
        let expected_type = TypeId::of::<W>();
        let mut planned_map = HashMap::with_capacity(self.map.len() + desired.len());
        for (key, typed_id) in &self.map {
            let node_id = NodeId::from(*typed_id);
            let Some(actual_type) = ctx.type_id_of(node_id) else {
                continue;
            };
            if actual_type != expected_type {
                return Err(Error::Invalid(
                    "keyed child points to a different widget type".into(),
                ));
            }
            if direct_children.contains(&node_id) {
                planned_map.insert(key.clone(), *typed_id);
            }
        }

        let managed: HashMap<NodeId, &K> = planned_map
            .iter()
            .map(|(key, id)| (NodeId::from(*id), key))
            .collect();
        if children.iter().any(|node| !managed.contains_key(node)) {
            return Err(Error::Invalid(
                "keyed collection parent contains unmanaged children".into(),
            ));
        }

        let mut candidates = Vec::new();
        for key in &desired {
            if !planned_map.contains_key(key) {
                candidates.push((key.clone(), create(key)?));
            }
        }

        let removed: Vec<K> = children
            .iter()
            .filter_map(|node_id| {
                let key = *managed.get(node_id)?;
                (!seen.contains(key)).then(|| key.clone())
            })
            .collect();
        let mut input = Some((planned_map, candidates));
        let mut output = None;
        ctx.edit_structure(&mut |ctx| {
            let (mut working_map, candidates) = input
                .take()
                .ok_or_else(|| Error::Internal("reconcile edit ran twice".into()))?;
            for (key, widget) in candidates {
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
                if ctx.type_id_of(node_id) != Some(expected_type) {
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
                if ctx.type_id_of(node_id).is_none() {
                    working_map.remove(key);
                    continue;
                }
                ctx.remove_subtree(node_id)?;
                working_map.remove(key);
            }

            for typed_id in &ordered {
                let node_id = NodeId::from(*typed_id);
                if ctx.type_id_of(node_id) != Some(expected_type) {
                    return Err(Error::Invalid(
                        "keyed child became stale before commit".into(),
                    ));
                }
            }

            let managed: HashSet<NodeId> =
                working_map.values().map(|id| NodeId::from(*id)).collect();
            if ctx
                .children_of(parent)
                .iter()
                .any(|node| !managed.contains(node))
            {
                return Err(Error::Invalid(
                    "keyed collection update inserted unmanaged children".into(),
                ));
            }
            let ordered_nodes = ordered.iter().map(|id| NodeId::from(*id)).collect();
            ctx.set_children(parent, ordered_nodes)?;
            output = Some((working_map, ordered));
            Ok(())
        })?;

        let (map, ordered) =
            output.ok_or_else(|| Error::Internal("reconcile edit never ran".into()))?;
        self.map = map;
        self.order = desired;
        Ok(ordered)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use canopy::{Canopy, CanopyBuilder};

    use super::*;

    /// A leaf that can fail its mount and logs its removal.
    struct Leaf {
        /// Name recorded by the removal hook.
        name: &'static str,
        /// Whether mounting fails.
        fail_mount: bool,
        /// Shared log of removed names.
        removed: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Leaf {
        fn new(name: &'static str, removed: &Rc<RefCell<Vec<&'static str>>>) -> Self {
            Self {
                name,
                fail_mount: false,
                removed: Rc::clone(removed),
            }
        }
    }

    impl Widget for Leaf {
        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            if self.fail_mount {
                return Err(Error::Invalid("mount failure".into()));
            }
            Ok(())
        }

        fn pre_remove(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            self.removed.borrow_mut().push(self.name);
            Ok(())
        }
    }

    /// Build an empty application and a shared removal log.
    fn setup() -> Result<(Canopy, Rc<RefCell<Vec<&'static str>>>)> {
        Ok((CanopyBuilder::new().build()?, Rc::default()))
    }

    /// Return the root's children.
    fn children(canopy: &mut Canopy) -> Result<Vec<NodeId>> {
        canopy.with_root_context(|ctx| Ok(ctx.children_of(ctx.node_id())))
    }

    #[test]
    fn rejects_unmanaged_children_before_callbacks() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<u32, Leaf>::new();
        let unmanaged = canopy.with_root_context(|ctx| {
            let unmanaged = ctx.add_child(ctx.node_id(), Leaf::new("unmanaged", &log))?;
            let result = keyed.reconcile(
                ctx,
                [1],
                |_| panic!("must reject before create"),
                |_, _, _| panic!("must reject before update"),
            );
            assert!(result.is_err());
            Ok(NodeId::from(unmanaged))
        })?;
        assert_eq!(children(&mut canopy)?, [unmanaged]);
        assert!(keyed.is_empty());
        Ok(())
    }

    #[test]
    fn rolls_back_an_unmanaged_insert_from_update() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<u32, Leaf>::new();
        let original = canopy.with_root_context(|ctx| {
            let original =
                keyed.reconcile(ctx, [1], |_| Ok(Leaf::new("one", &log)), |_, _, _| Ok(()))?;
            let result = keyed.reconcile(
                ctx,
                [1],
                |_| Ok(Leaf::new("one", &log)),
                |_, _, ctx| {
                    let root = ctx.root_id();
                    ctx.add_child(root, Leaf::new("extra", &log)).map(|_| ())
                },
            );
            assert!(result.is_err());
            Ok(original)
        })?;
        assert_eq!(children(&mut canopy)?, [NodeId::from(original[0])]);
        assert_eq!(keyed.id_for(&1), Some(original[0]));
        Ok(())
    }

    #[test]
    fn removes_in_current_child_order() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<&'static str, Leaf>::new();
        let (rows, retained) = canopy.with_root_context(|ctx| {
            let rows = keyed.reconcile(
                ctx,
                ["a", "b", "c", "d"],
                |key| Ok(Leaf::new(key, &log)),
                |_, _, _| Ok(()),
            )?;
            let order = [2, 0, 3, 1].map(|index| rows[index].into()).to_vec();
            ctx.set_children(ctx.node_id(), order)?;
            let retained = keyed.reconcile(
                ctx,
                ["b"],
                |_| panic!("all requested keys already exist"),
                |_, _, _| Ok(()),
            )?;
            Ok((rows, retained))
        })?;
        assert_eq!(retained, [rows[1]]);
        assert_eq!(*log.borrow(), ["c", "a", "d"]);
        Ok(())
    }

    #[test]
    fn update_failure_leaves_tree_and_collection_unchanged() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<&'static str, Leaf>::new();
        canopy.with_root_context(|ctx| {
            let error = keyed
                .reconcile(
                    ctx,
                    ["a", "b"],
                    |key| Ok(Leaf::new(key, &log)),
                    |key, id, ctx| {
                        if *key == "a" {
                            ctx.set_hidden(id.into(), true).map(|_| ())
                        } else {
                            Err(Error::Invalid("update failure".into()))
                        }
                    },
                )
                .expect_err("update failure aborts the reconcile");
            assert!(matches!(error, Error::Invalid(_)));
            Ok(())
        })?;
        assert!(children(&mut canopy)?.is_empty());
        assert!(keyed.is_empty());
        Ok(())
    }

    #[test]
    fn mount_failure_leaves_tree_and_collection_unchanged() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<&'static str, Leaf>::new();
        canopy.with_root_context(|ctx| {
            let error = keyed
                .reconcile(
                    ctx,
                    ["a", "b"],
                    |key| {
                        let mut leaf = Leaf::new(key, &log);
                        leaf.fail_mount = *key == "b";
                        Ok(leaf)
                    },
                    |_, _, _| Ok(()),
                )
                .expect_err("mount failure aborts the reconcile");
            assert!(matches!(error, Error::Invalid(_)));
            Ok(())
        })?;
        assert!(children(&mut canopy)?.is_empty());
        assert!(keyed.is_empty());
        Ok(())
    }

    #[test]
    fn defers_removal_until_updates_succeed() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<&'static str, Leaf>::new();
        let before = canopy.with_root_context(|ctx| {
            keyed.reconcile(
                ctx,
                ["a", "b"],
                |key| Ok(Leaf::new(key, &log)),
                |_, _, _| Ok(()),
            )
        })?;
        canopy.with_root_context(|ctx| {
            keyed
                .reconcile(
                    ctx,
                    ["b", "c"],
                    |key| Ok(Leaf::new(key, &log)),
                    |key, _, _| {
                        if *key == "c" {
                            Err(Error::Invalid("late update failure".into()))
                        } else {
                            Ok(())
                        }
                    },
                )
                .expect_err("a late update failure aborts the reconcile");
            Ok(())
        })?;
        let before: Vec<NodeId> = before.into_iter().map(NodeId::from).collect();
        assert_eq!(children(&mut canopy)?, before);
        assert_eq!(keyed.keys(), &["a", "b"]);
        assert!(log.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn preserves_ids_when_reordered_and_rejects_duplicates_first() -> Result<()> {
        let (mut canopy, log) = setup()?;
        let mut keyed = KeyedChildren::<&'static str, Leaf>::new();
        canopy.with_root_context(|ctx| {
            let first = keyed.reconcile(
                ctx,
                ["a", "b"],
                |key| Ok(Leaf::new(key, &log)),
                |_, _, _| Ok(()),
            )?;
            let second = keyed.reconcile(
                ctx,
                ["b", "a"],
                |_| panic!("reordered keys reuse their widgets"),
                |_, _, _| Ok(()),
            )?;
            assert_eq!(second, [first[1], first[0]]);
            let second: Vec<NodeId> = second.into_iter().map(NodeId::from).collect();
            assert_eq!(ctx.children_of(ctx.node_id()), second);
            let error = keyed
                .reconcile(
                    ctx,
                    ["a", "a"],
                    |_| panic!("duplicates fail before creation"),
                    |_, _, _| panic!("duplicates fail before update"),
                )
                .expect_err("duplicate keys fail");
            assert!(matches!(error, Error::Invalid(_)));
            Ok(())
        })?;
        assert_eq!(keyed.keys(), &["b", "a"]);
        Ok(())
    }
}
