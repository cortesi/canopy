//! Modal input admission and token-owned interaction state.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{Core, focus::is_focus_candidate};
use crate::{
    FrameworkBindingGroup, Invalidation, NodeId,
    error::{Error, Result},
    style::effects::{self, Effect},
};

/// Opaque identity of one modal scope, unique across applications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionToken(u64);

/// Bindings admitted within a modal's route to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalBindings {
    /// Admit only this framework binding group.
    Framework(FrameworkBindingGroup),
    /// Admit ordinary application bindings on the bounded modal route.
    Application,
}

/// Nodes and binding admission owned by one modal scope.
#[derive(Clone, Copy, Debug)]
pub struct ModalOptions {
    /// Ancestor that owns the modal lifetime and bounds binding routing.
    pub owner: NodeId,
    /// Subtree that receives normal input while this scope is on top.
    pub modal: NodeId,
    /// Focusable node inside the modal to focus on successful open.
    pub initial_focus: NodeId,
    /// Optional subtree dimmed while the scope remains active.
    pub dim_target: Option<NodeId>,
    /// Binding ownership admitted by this scope.
    pub bindings: ModalBindings,
}

/// Node identity that cannot silently refer to a replacement widget.
#[derive(Clone, Copy)]
struct Identity {
    /// Arena identity.
    node: NodeId,
    /// Widget generation.
    incarnation: u64,
}

/// Owned state retained until a scope is closed or structurally retired.
#[derive(Clone)]
struct ModalScope {
    /// Stable public handle.
    token: InteractionToken,
    /// Declared input and visual behavior.
    options: ModalOptions,
    /// Owning widget lifetime.
    owner: Identity,
    /// Modal widget lifetime.
    modal: Identity,
    /// Dimming target lifetime, independent from its ordinary effects.
    dim: Option<Identity>,
    /// Exact prior focus followed by its nearest ancestors.
    focus_ancestry: Vec<Identity>,
}

/// Stack of admitted modal scopes, included in structural rollback snapshots.
#[derive(Clone, Default)]
pub(super) struct InteractionState {
    /// Last scope is the sole normal input region.
    scopes: Vec<ModalScope>,
}

impl Core {
    /// Capture a live node's widget identity.
    fn interaction_identity(&self, node: NodeId) -> Result<Identity> {
        self.validate_attached_node(node)?;
        Ok(Identity {
            node,
            incarnation: self.nodes[node].incarnation,
        })
    }

    /// Whether a saved identity still belongs to the active tree.
    fn interaction_identity_live(&self, identity: Identity) -> bool {
        self.nodes
            .get(identity.node)
            .is_some_and(|entry| entry.incarnation == identity.incarnation)
            && self.is_attached_to_root(identity.node)
    }

    /// Whether a token still owns a scope, including a close awaiting
    /// completion.
    pub(crate) fn modal_is_open(&self, token: InteractionToken) -> bool {
        self.interaction
            .scopes
            .iter()
            .any(|scope| scope.token == token)
    }

    /// Return the top normal-input subtree, if any.
    pub(crate) fn modal_region(&self) -> Option<NodeId> {
        self.interaction
            .scopes
            .last()
            .map(|scope| scope.options.modal)
    }

    /// Return the last ancestor eligible for modal binding routing.
    pub(crate) fn modal_owner(&self) -> Option<NodeId> {
        self.interaction
            .scopes
            .last()
            .map(|scope| scope.options.owner)
    }

    /// Whether normal input, focus, and capture may target this node.
    pub(crate) fn interaction_admits(&self, node: NodeId) -> bool {
        self.modal_region()
            .is_none_or(|modal| self.is_ancestor_or_self(modal, node))
    }

    /// Return effects owned by scopes without altering widget-owned effects.
    pub(crate) fn modal_effects_for(&self, node: NodeId) -> Vec<Effect> {
        self.interaction
            .scopes
            .iter()
            .filter(|scope| {
                scope.dim.is_some_and(|identity| {
                    identity.node == node && self.interaction_identity_live(identity)
                })
            })
            .map(|_| effects::brightness(0.5))
            .collect()
    }

    /// Synchronize the binding map with the top scope after a stack change.
    pub(crate) fn sync_modal_bindings(&mut self) {
        self.input_map.set_modal_bindings(
            self.interaction
                .scopes
                .last()
                .map(|scope| scope.options.bindings),
        );
    }

    /// Open one modal transaction, preserving prior interaction on failure.
    pub(crate) fn open_modal(&mut self, options: ModalOptions) -> Result<InteractionToken> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = self.interaction_identity(options.owner)?;
        let modal = self.interaction_identity(options.modal)?;
        self.validate_attached_node(options.initial_focus)?;
        let dim = options
            .dim_target
            .map(|node| self.interaction_identity(node))
            .transpose()?;
        if !self.is_ancestor_or_self(options.owner, options.modal)
            || !self.is_ancestor_or_self(options.modal, options.initial_focus)
            || !self.interaction_admits(options.modal)
            || !self.interaction_admits(options.owner)
            || self
                .interaction
                .scopes
                .iter()
                .any(|scope| scope.options.modal == options.modal)
        {
            return Err(Error::Invalid(
                "modal owner, subtree, and focus must form an admitted route".into(),
            ));
        }
        let mut focus_ancestry = Vec::new();
        let mut current = self.focus;
        while let Some(node) = current {
            focus_ancestry.push(self.interaction_identity(node)?);
            current = self.nodes[node].parent;
        }
        let token = InteractionToken(
            NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| Error::InvalidOperation("interaction token space exhausted".into()))?,
        );
        let hidden = self.nodes[options.modal].hidden;
        let focus = self.focus;
        let capture = self.mouse_capture;
        self.nodes[options.modal].hidden = false;
        self.mouse_capture = None;
        self.interaction.scopes.push(ModalScope {
            token,
            options,
            owner,
            modal,
            dim,
            focus_ancestry,
        });
        self.sync_modal_bindings();
        let result = if is_focus_candidate(self, options.initial_focus, false) {
            self.set_focus(options.initial_focus).map(|_| ())
        } else {
            Err(Error::Invalid(
                "initial modal focus does not accept focus or is hidden".into(),
            ))
        };
        if let Err(error) = result {
            self.interaction.scopes.pop();
            self.sync_modal_bindings();
            self.nodes[options.modal].hidden = hidden;
            self.focus = focus;
            self.mouse_capture = capture;
            return Err(error);
        }
        self.invalidate(Invalidation::Layout);
        Ok(token)
    }

    /// Close at the shared callback completion boundary.
    pub(crate) fn close_modal(&mut self, token: InteractionToken) -> Result<()> {
        self.close_modal_after_dispatch(token)
    }

    /// Close the requested scope and all younger scopes after callbacks return.
    pub(crate) fn close_modal_now(&mut self, token: InteractionToken) -> Result<()> {
        let Some(index) = self
            .interaction
            .scopes
            .iter()
            .position(|scope| scope.token == token)
        else {
            return Ok(());
        };
        let retired: Vec<_> = self.interaction.scopes.drain(index..).collect();
        self.sync_modal_bindings();
        self.mouse_capture = None;
        for scope in retired.iter().rev() {
            if self.interaction_identity_live(scope.modal) {
                self.nodes[scope.modal.node].hidden = true;
            }
        }
        self.invalidate(Invalidation::Layout);
        let ancestry = &retired[0].focus_ancestry;
        let exact = ancestry
            .first()
            .filter(|identity| {
                self.interaction_identity_live(**identity)
                    && self.interaction_admits(identity.node)
                    && is_focus_candidate(self, identity.node, false)
            })
            .map(|identity| identity.node);
        let fallback = || {
            ancestry
                .iter()
                .filter(|identity| self.interaction_identity_live(**identity))
                .find_map(|identity| {
                    self.subtree_pre_order(identity.node)
                        .into_iter()
                        .find(|node| {
                            self.interaction_admits(*node) && is_focus_candidate(self, *node, false)
                        })
                })
        };
        let focus = exact.or_else(fallback).or_else(|| {
            let root = self.modal_region().unwrap_or(self.root);
            self.subtree_pre_order(root)
                .into_iter()
                .find(|node| is_focus_candidate(self, *node, false))
        });
        self.transition_focus(focus)?;
        Ok(())
    }

    /// Retire scopes invalidated by a successful structural commit.
    pub(crate) fn retire_invalid_interactions(&mut self) -> Result<()> {
        let invalid = self
            .interaction
            .scopes
            .iter()
            .enumerate()
            .position(|(index, scope)| {
                !self.interaction_identity_live(scope.owner)
                    || !self.interaction_identity_live(scope.modal)
                    || !self.is_ancestor_or_self(scope.options.owner, scope.options.modal)
                    || (index > 0
                        && !self.is_ancestor_or_self(
                            self.interaction.scopes[index - 1].options.modal,
                            scope.options.owner,
                        ))
            });
        if let Some(index) = invalid {
            self.close_modal_now(self.interaction.scopes[index].token)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        event::{Event, key, mouse},
        testing::ttree::{Bb, get_state, reset_state, run_ttree},
    };

    #[test]
    fn modal_input_skips_owner_widgets_and_consumes_outside_capture_events() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            canopy.core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: None,
                bindings: ModalBindings::Application,
            })?;
            reset_state();
            canopy.event(&Event::Key('z'.into()))?;
            assert_eq!(
                get_state().path.len(),
                2,
                "only leaf and modal widgets receive the key"
            );
            assert!(
                get_state()
                    .path
                    .iter()
                    .all(|event| !event.starts_with("r@"))
            );
            canopy.core.capture_mouse(tree.b_a)?;
            let location = canopy.core.nodes[tree.a_a].view.content.tl;
            reset_state();
            canopy.event(&Event::Mouse(mouse::MouseEvent {
                action: mouse::Action::Down,
                button: mouse::Button::Left,
                modifiers: key::Empty,
                location,
            }))?;
            assert!(
                get_state().path.is_empty(),
                "outside input must not reach capture or background"
            );
            Ok(())
        })
    }

    #[test]
    fn failed_open_restores_capture_focus_visibility_and_admission() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let core = &mut canopy.core;
            core.set_focus(tree.a_a)?;
            core.capture_mouse(tree.a_a)?;
            core.set_hidden(tree.b, true)?;
            core.set_hidden(tree.b_a, true)?;
            let result = core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: Some(tree.a),
                bindings: ModalBindings::Application,
            });
            assert!(result.is_err());
            assert_eq!(core.focus, Some(tree.a_a));
            assert_eq!(core.mouse_capture, Some(tree.a_a));
            assert!(core.nodes[tree.b].hidden);
            assert_eq!(core.modal_region(), None);
            assert!(core.modal_effects_for(tree.a).is_empty());
            Ok(())
        })
    }

    #[test]
    fn nested_close_restores_focus_and_keeps_unrelated_effects() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let core = &mut canopy.core;
            core.set_focus(tree.a_a)?;
            core.capture_mouse(tree.a_a)?;
            let ordinary = effects::brightness(0.8);
            core.nodes[tree.a].effects.push(Arc::clone(&ordinary));
            let outer = core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: Some(tree.a),
                bindings: ModalBindings::Application,
            })?;
            assert_eq!(core.mouse_capture, None);
            assert!(core.set_focus(tree.a_a).is_err());
            assert!(core.capture_mouse(tree.a_a).is_err());
            let inner = core.open_modal(ModalOptions {
                owner: tree.b,
                modal: tree.b_b,
                initial_focus: tree.b_b,
                dim_target: Some(tree.b_a),
                bindings: ModalBindings::Application,
            })?;
            core.close_modal(inner)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            assert_eq!(core.focus, Some(tree.b_a));
            let _inner = core.open_modal(ModalOptions {
                owner: tree.b,
                modal: tree.b_b,
                initial_focus: tree.b_b,
                dim_target: Some(tree.b_a),
                bindings: ModalBindings::Application,
            })?;
            core.close_modal(outer)?;
            assert_eq!(core.modal_region(), None);
            assert_eq!(core.focus, Some(tree.a_a));
            assert_eq!(core.mouse_capture, None);
            assert!(core.nodes[tree.b].hidden);
            assert!(core.nodes[tree.b_b].hidden);
            assert!(core.modal_effects_for(tree.a).is_empty());
            assert_eq!(core.nodes[tree.a].effects.len(), 1);
            assert!(Arc::ptr_eq(&ordinary, &core.nodes[tree.a].effects[0]));
            core.close_modal(outer)?;
            Ok(())
        })
    }

    #[test]
    fn close_uses_dispatch_checkpoint_and_replacement_retires_scope() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let core = &mut canopy.core;
            core.set_focus(tree.a_a)?;
            let token = core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: Some(tree.a),
                bindings: ModalBindings::Application,
            })?;
            let checkpoint = core.begin_dispatch();
            core.close_modal(token)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            core.finish_dispatch(checkpoint, false)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            let checkpoint = core.begin_dispatch();
            core.close_modal(token)?;
            assert!(!core.nodes[tree.b].hidden);
            core.finish_dispatch(checkpoint, true)?;
            assert!(core.nodes[tree.b].hidden);
            let _token = core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: Some(tree.a),
                bindings: ModalBindings::Application,
            })?;
            core.replace_subtree(tree.b, Bb::new())?;
            assert_eq!(core.modal_region(), None);
            assert!(core.modal_effects_for(tree.a).is_empty());
            assert!(
                !core.nodes[tree.b].hidden,
                "retirement must not hide the replacement widget"
            );
            Ok(())
        })
    }

    #[test]
    fn removed_origin_focus_falls_back_inside_surviving_ancestor() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let core = &mut canopy.core;
            core.set_focus(tree.a_a)?;
            let token = core.open_modal(ModalOptions {
                owner: tree.root,
                modal: tree.b,
                initial_focus: tree.b_a,
                dim_target: None,
                bindings: ModalBindings::Application,
            })?;
            core.remove_subtree(tree.a_a)?;
            core.close_modal(token)?;
            assert_eq!(core.focus, Some(tree.a));
            Ok(())
        })
    }
}
