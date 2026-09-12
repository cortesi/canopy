//! Helpers for managing keyed child collections.

use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::{
    Context, ContextExt, NodeId, TypedId, Widget,
    error::{Error, Result},
    layout::LayoutOverride,
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
        let children = ctx.children();
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
        let mut planned_map = Some(planned_map);
        let mut candidates = Some(candidates);
        let mut outcome = None;

        ctx.edit_structure(&mut |ctx| {
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

/// Restricted builder for new children of one existing parent.
///
/// Configuration completes while nodes are detached. The enclosing structural
/// edit attaches roots in source order, then registers scoped semantic keys.
pub struct ChildBuilder<'a> {
    /// Context used only to configure newly created nodes.
    ctx: &'a mut dyn Context,
    /// Existing parent that receives the completed roots.
    parent: NodeId,
    /// Configured roots in source order with optional structural keys.
    roots: Vec<(NodeId, Option<&'static str>)>,
    /// Registrations deferred until scope membership is established.
    semantic_keys: Vec<(NodeId, NodeId, String)>,
}

/// Configuration access limited to a newly created detached node.
pub struct ChildConfig<'a, W> {
    /// Context restricted by this facade to the new subtree.
    ctx: &'a mut dyn Context,
    /// Newly created node being configured.
    id: TypedId<W>,
    /// Composition-wide registrations applied before mount.
    semantic_keys: &'a mut Vec<(NodeId, NodeId, String)>,
}

/// Create and configure one node while it remains detached.
fn configured_child<W: Widget + 'static>(
    ctx: &mut dyn Context,
    semantic_keys: &mut Vec<(NodeId, NodeId, String)>,
    widget: W,
    configure: impl FnOnce(&mut ChildConfig<'_, W>) -> Result<()>,
) -> Result<TypedId<W>> {
    let id = ctx.create_detached(widget)?;
    configure(&mut ChildConfig {
        ctx,
        id,
        semantic_keys,
    })?;
    Ok(id)
}

impl ChildBuilder<'_> {
    /// Configure an unkeyed child before attaching it to the existing parent.
    pub fn child<W: Widget + 'static>(
        &mut self,
        widget: W,
        configure: impl FnOnce(&mut ChildConfig<'_, W>) -> Result<()>,
    ) -> Result<TypedId<W>> {
        let id = configured_child(self.ctx, &mut self.semantic_keys, widget, configure)?;
        self.roots.push((id.into(), None));
        Ok(id)
    }

    /// Configure a child occupying a typed structural slot.
    pub fn keyed<K: crate::ChildSlot>(
        &mut self,
        widget: K::Widget,
        configure: impl FnOnce(&mut ChildConfig<'_, K::Widget>) -> Result<()>,
    ) -> Result<TypedId<K::Widget>> {
        if self.ctx.child_slot_of(self.parent, K::KEY).is_some()
            || self.roots.iter().any(|(_, key)| *key == Some(K::KEY))
        {
            return Err(Error::Invalid(format!("duplicate child key {:?}", K::KEY)));
        }
        let id = configured_child(self.ctx, &mut self.semantic_keys, widget, configure)?;
        self.roots.push((id.into(), Some(K::KEY)));
        Ok(id)
    }
}

impl<W: Widget + 'static> ChildConfig<'_, W> {
    /// Return the new node's typed identity.
    pub fn id(&self) -> TypedId<W> {
        self.id
    }

    /// Set persistent layout constraints before mount.
    pub fn layout_override(&mut self, overrides: LayoutOverride) -> Result<()> {
        self.ctx.set_layout_override_of(self.id.into(), overrides)
    }

    /// Register this node's semantic key after its completed tree is attached.
    pub fn semantic_key(&mut self, scope: NodeId, key: &str) -> Result<()> {
        if self.ctx.type_id_of(scope).is_none() {
            return Err(Error::NodeNotFound(scope));
        }
        self.semantic_keys
            .retain(|(node, _, _)| *node != NodeId::from(self.id));
        self.semantic_keys.push((self.id.into(), scope, key.into()));
        Ok(())
    }

    /// Configure and attach a descendant to this new detached parent.
    pub fn child<C: Widget + 'static>(
        &mut self,
        widget: C,
        configure: impl FnOnce(&mut ChildConfig<'_, C>) -> Result<()>,
    ) -> Result<TypedId<C>> {
        let id = configured_child(self.ctx, self.semantic_keys, widget, configure)?;
        self.ctx.attach(self.id.into(), id.into())?;
        Ok(id)
    }

    /// Configure a descendant in a typed structural slot.
    pub fn keyed<K: crate::ChildSlot>(
        &mut self,
        widget: K::Widget,
        configure: impl FnOnce(&mut ChildConfig<'_, K::Widget>) -> Result<()>,
    ) -> Result<TypedId<K::Widget>> {
        if self.ctx.child_slot_of(self.id.into(), K::KEY).is_some() {
            return Err(Error::Invalid(format!("duplicate child key {:?}", K::KEY)));
        }
        let id = configured_child(self.ctx, self.semantic_keys, widget, configure)?;
        self.ctx.attach_slot(self.id.into(), K::KEY, id.into())?;
        Ok(id)
    }
}

/// Build new children atomically through a restricted detached-node facade.
///
/// Structural rollback follows [`Context::edit_structure`]; external
/// effects of configuration closures are not rolled back.
pub(super) fn compose<R>(
    context: &mut dyn Context,
    parent: NodeId,
    build: impl FnOnce(&mut ChildBuilder<'_>) -> Result<R>,
) -> Result<R> {
    if context.type_id_of(parent).is_none() {
        return Err(Error::NodeNotFound(parent));
    }
    let mut build = Some(build);
    let mut result = None;
    context.edit_structure(&mut |ctx| {
        let mut builder = ChildBuilder {
            ctx,
            parent,
            roots: Vec::new(),
            semantic_keys: Vec::new(),
        };
        let output = build
            .take()
            .ok_or_else(|| Error::Internal("composition already consumed".into()))?(
            &mut builder
        )?;
        super::context::sealed::Context::attach_composed(
            builder.ctx,
            parent,
            &builder.roots,
            &builder.semantic_keys,
        )?;
        result = Some(output);
        Ok(())
    })?;
    result.ok_or_else(|| Error::Internal("composition result missing".into()))
}
#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::{
        ViewContext,
        core::{context::CoreContext, world::Core},
        layout::LayoutOverride,
    };

    struct Leaf;
    impl Widget for Leaf {}
    crate::slot!(Slot: Leaf);

    struct Mounted {
        label: &'static str,
        scope: NodeId,
        log: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Widget for Mounted {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            assert_eq!(
                ctx.find_identity(self.scope, self.label)?,
                Some(ctx.node_id())
            );
            assert_eq!(ctx.layout().max_height, Some(3));
            if self.label == "parent" {
                assert_eq!(ctx.children().len(), 1);
            }
            self.log.borrow_mut().push(self.label);
            Ok(())
        }
    }

    #[test]
    fn composition_mounts_completed_topology_layout_and_semantics_in_source_order() -> Result<()> {
        let mut core = Core::new();
        let root = core.root_id();
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut context = CoreContext::new(&mut core, root);
        let ctx: &mut dyn Context = &mut context;
        let parent = ctx.compose(root, |builder| {
            let parent = builder.child(
                Mounted {
                    label: "parent",
                    scope: root,
                    log: log.clone(),
                },
                |parent| {
                    parent.layout_override(LayoutOverride::new().fixed_height(3))?;
                    parent.semantic_key(root, "parent")?;
                    parent.child(
                        Mounted {
                            label: "nested",
                            scope: root,
                            log: log.clone(),
                        },
                        |nested| {
                            nested.layout_override(LayoutOverride::new().fixed_height(3))?;
                            nested.semantic_key(root, "nested")
                        },
                    )?;
                    Ok(())
                },
            )?;
            builder.child(
                Mounted {
                    label: "sibling",
                    scope: root,
                    log: log.clone(),
                },
                |child| {
                    child.layout_override(LayoutOverride::new().fixed_height(3))?;
                    child.semantic_key(root, "sibling")
                },
            )?;
            Ok(parent)
        })?;
        assert_eq!(*log.borrow(), ["parent", "nested", "sibling"]);
        assert_eq!(ctx.children()[0], NodeId::from(parent));
        Ok(())
    }

    #[test]
    fn failed_composition_restores_nodes_and_structural_keys() -> Result<()> {
        let mut core = Core::new();
        let root = core.root_id();
        let mut context = CoreContext::new(&mut core, root);
        let ctx: &mut dyn Context = &mut context;
        let before = ctx.children();
        let mut created = None;
        let result = ctx.compose(root, |builder| {
            builder.keyed::<Slot>(Leaf, |child| {
                created = Some(child.id());
                Ok(())
            })?;
            builder.keyed::<Slot>(Leaf, |_| Ok(()))?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(ctx.children(), before);
        assert!(ctx.child_slot("Slot").is_none());
        assert!(ctx.type_id_of(created.unwrap().into()).is_none());
        let result: Result<()> = ctx.compose(root, |builder| {
            builder.child(Leaf, |child| {
                child.child(Leaf, |_| Err(Error::Invalid("configuration failed".into())))?;
                Ok(())
            })?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(ctx.children(), before);
        Ok(())
    }

    #[test]
    fn keyed_collection_rejects_unmanaged_children_before_callbacks() -> Result<()> {
        let mut core = Core::new();
        let root = core.root_id();
        let unmanaged = core.create_detached(Leaf)?;
        core.attach(root, unmanaged)?;
        let mut collection = KeyedChildren::<u32, Leaf>::new();
        let mut context = CoreContext::new(&mut core, root);
        let result = collection.reconcile(
            &mut context,
            [1],
            |_| panic!("must reject before create"),
            |_, _, _| panic!("must reject before update"),
        );
        assert!(result.is_err());
        assert_eq!(context.children(), [unmanaged]);
        assert!(collection.is_empty());
        Ok(())
    }

    #[test]
    fn keyed_collection_rolls_back_unmanaged_insert_from_update() -> Result<()> {
        let mut core = Core::new();
        let root = core.root_id();
        let mut context = CoreContext::new(&mut core, root);
        let mut collection = KeyedChildren::<u32, Leaf>::new();
        let original = collection.reconcile(&mut context, [1], |_| Ok(Leaf), |_, _, _| Ok(()))?;
        let result = collection.reconcile(
            &mut context,
            [1],
            |_| Ok(Leaf),
            |_, _, ctx| {
                let extra = ctx.create_detached(Leaf)?;
                ctx.attach(root, extra.into())
            },
        );
        assert!(result.is_err());
        assert_eq!(context.children(), [NodeId::from(original[0])]);
        assert_eq!(collection.id_for(&1), Some(original[0]));
        Ok(())
    }
}
