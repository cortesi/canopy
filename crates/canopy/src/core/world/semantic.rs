//! Explicit semantic identity scoped to an arena subtree.

use super::Core;
use crate::{
    Invalidation, NodeId, SemanticIdentity,
    error::{Error, Result},
};

impl Core {
    /// Assign an identity after validating its scope and uniqueness.
    pub(crate) fn set_semantic_key(
        &mut self,
        node: NodeId,
        scope: NodeId,
        key: &str,
    ) -> Result<()> {
        if !self.nodes.contains_key(node) {
            return Err(Error::NodeNotFound(node));
        }
        if !self.nodes.contains_key(scope) {
            return Err(Error::NodeNotFound(scope));
        }
        if !self.is_ancestor_or_self(scope, node) {
            return Err(Error::Invalid(
                "semantic key owner must belong to its scope subtree".into(),
            ));
        }
        let identity = SemanticIdentity {
            scope,
            key: key.into(),
        };
        if self.nodes[node].semantic_identity.as_ref() == Some(&identity) {
            return Ok(());
        }
        if self
            .semantic_keys
            .get(&(scope, key.into()))
            .is_some_and(|other| *other != node && self.nodes.contains_key(*other))
        {
            return Err(Error::Invalid(format!(
                "duplicate semantic key {key:?} in scope {scope:?}"
            )));
        }
        self.clear_semantic_key(node)?;
        self.semantic_keys.insert((scope, key.into()), node);
        self.nodes[node].semantic_identity = Some(identity);
        self.invalidate(Invalidation::Semantics);
        Ok(())
    }

    /// Clear one node's registration without changing its structural child key.
    pub(crate) fn clear_semantic_key(&mut self, node: NodeId) -> Result<()> {
        let entry = self.nodes.get_mut(node).ok_or(Error::NodeNotFound(node))?;
        if let Some(identity) = entry.semantic_identity.take() {
            self.semantic_keys.remove(&(identity.scope, identity.key));
            self.invalidate(Invalidation::Semantics);
        }
        Ok(())
    }

    /// Find a registered key within an explicit live arena scope.
    pub(crate) fn find_key(&self, scope: NodeId, key: &str) -> Result<Option<NodeId>> {
        if !self.nodes.contains_key(scope) {
            return Err(Error::NodeNotFound(scope));
        }
        Ok(self
            .semantic_keys
            .get(&(scope, key.into()))
            .copied()
            .filter(|node| self.nodes.contains_key(*node)))
    }

    /// Retire identities that no longer belong to their scope after an outer
    /// edit.
    pub(super) fn prune_semantic_keys(&mut self) {
        let retired: Vec<_> = self
            .nodes
            .iter()
            .filter_map(|(node, entry)| {
                entry
                    .semantic_identity
                    .as_ref()
                    .filter(|identity| {
                        !self.nodes.contains_key(identity.scope)
                            || !self.is_ancestor_or_self(identity.scope, node)
                    })
                    .map(|_| node)
            })
            .collect();
        for node in retired {
            self.nodes[node].semantic_identity = None;
        }
        self.semantic_keys.retain(|(scope, key), node| {
            self.nodes
                .get(*node)
                .and_then(|entry| entry.semantic_identity.as_ref())
                .is_some_and(|identity| identity.scope == *scope && identity.key == *key)
        });
    }

    /// Check the bidirectional identity index and settled scope membership.
    pub(super) fn validate_semantic_keys(&self) -> Result<()> {
        for (node, entry) in &self.nodes {
            if let Some(identity) = &entry.semantic_identity {
                if self
                    .semantic_keys
                    .get(&(identity.scope, identity.key.clone()))
                    != Some(&node)
                {
                    return Err(Error::Invariant(
                        "semantic identity is missing from its scope index".into(),
                    ));
                }
                if self.tree_edit.is_none() && !self.is_ancestor_or_self(identity.scope, node) {
                    return Err(Error::Invariant(
                        "semantic identity lies outside its scope subtree".into(),
                    ));
                }
            }
        }
        for ((scope, key), node) in &self.semantic_keys {
            if !self
                .nodes
                .get(*node)
                .and_then(|entry| entry.semantic_identity.as_ref())
                .is_some_and(|identity| identity.scope == *scope && identity.key == *key)
            {
                return Err(Error::Invariant(
                    "semantic scope index has no matching node identity".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, ViewContext, core::context::CoreContext, widget::Widget};

    struct Leaf;
    impl Widget for Leaf {}

    #[test]
    fn scoped_identity_tracks_moves_replacement_and_nested_rollback() -> Result<()> {
        let mut core = Core::new();
        let root = core.root_id();
        let a = core.create_detached(Leaf)?;
        let b = core.create_detached(Leaf)?;
        core.attach(root, a)?;
        core.attach(root, b)?;
        let child = core.create_detached(Leaf)?;
        core.attach(a, child)?;
        core.set_semantic_key(child, a, "field")?;
        core.set_semantic_key(b, b, "field")?;
        let duplicate = core.create_detached(Leaf)?;
        core.attach(a, duplicate)?;
        assert!(core.set_semantic_key(duplicate, a, "field").is_err());
        assert!(core.set_semantic_key(child, b, "outside").is_err());
        core.with_tree_edit("wrap", |core| {
            let wrapper = core.create_detached(Leaf)?;
            core.detach(child)?;
            core.attach(wrapper, child)?;
            core.attach(a, wrapper)?;
            Ok(())
        })?;
        assert_eq!(core.find_key(a, "field")?, Some(child));
        let failure: Result<()> = core.with_tree_edit("outer failure", |core| {
            core.detach(child)?;
            core.attach(b, child)?;
            let nested: Result<()> = core.with_tree_edit("inner failure", |core| {
                core.clear_semantic_key(child)?;
                core.set_semantic_key(child, b, "provisional")?;
                Err(Error::Invalid("rollback inner".into()))
            });
            assert!(nested.is_err());
            assert_eq!(core.find_key(a, "field")?, Some(child));
            assert_eq!(core.find_key(b, "provisional")?, None);
            Err(Error::Invalid("rollback outer".into()))
        });
        assert!(failure.is_err());
        assert_eq!(core.find_key(a, "field")?, Some(child));
        core.with_tree_edit("move", |core| {
            core.detach(child)?;
            core.attach(b, child)
        })?;
        assert_eq!(core.find_key(a, "field")?, None);
        core.set_semantic_key(child, b, "child")?;
        core.replace_subtree(child, Leaf)?;
        assert_eq!(core.find_key(b, "child")?, None);
        core.set_semantic_key(child, b, "child")?;
        core.remove_subtree(child)?;
        assert_eq!(core.find_key(b, "child")?, None);
        core.validate_semantic_keys()
    }

    #[test]
    fn detached_internal_scope_survives_attachment_and_clear_is_independent() -> Result<()> {
        let mut core = Core::new();
        let scope = core.create_detached(Leaf)?;
        let child = core.create_detached(Leaf)?;
        core.attach_keyed(scope, "slot", child)?;
        let root = core.root_id();
        {
            let mut context = CoreContext::new(&mut core, scope);
            context.set_semantic_key(child, scope, "field")?;
            assert_eq!(
                context.semantic_identity(child),
                Some(SemanticIdentity {
                    scope,
                    key: "field".into()
                })
            );
            context.attach(root, scope)?;
            assert_eq!(context.find_key(scope, "field")?, Some(child));
            context.clear_semantic_key(child)?;
            assert_eq!(context.find_key(scope, "field")?, None);
            assert_eq!(context.child_keyed_in(scope, "slot"), Some(child));
        }
        core.validate_semantic_keys()
    }
}
