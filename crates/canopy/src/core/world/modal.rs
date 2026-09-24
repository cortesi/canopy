//! Modal input admission and the stack of open modals.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{Core, focus::is_focus_candidate};
use crate::{
    Invalidation, NodeId,
    error::{Error, Result},
    input::FrameworkBindingGroup,
    path::Path,
    style::{effects, effects::Effect},
};

/// Opaque identity of one modal, unique across applications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModalToken(u64);

/// Bindings admitted within a modal's route to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalBindings {
    /// Admit these framework groups first, then every global binding and
    /// application bindings to the listed intents, on the bounded modal route.
    ///
    /// A host admits its own group and the groups the widgets inside the
    /// modal ship, such as [`Confirm::BINDINGS`](../../canopy_widgets). The
    /// first group names the modal in binding snapshots.
    Framework {
        /// Framework groups the modal admits, the owner's first.
        groups: &'static [FrameworkBindingGroup],
        /// Exact intent names the modal admits from application bindings.
        intents: &'static [&'static str],
    },
    /// Admit ordinary application bindings on the bounded modal route.
    Application,
}

/// Nodes and binding admission owned by one modal.
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

/// The input route from a start node toward the root.
///
/// Built by [`Core::route`].
pub struct Route<'a> {
    /// Tree the route walks.
    core: &'a Core,
    /// Next node to yield, if the route continues.
    next: Option<NodeId>,
    /// Path from the root to `next`.
    path: Path,
}

impl Iterator for Route<'_> {
    type Item = (NodeId, Path);

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.next?;
        let path = self.path.clone();
        self.next = self.core.route_step(node);
        self.path.pop();
        Some((node, path))
    }
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
    token: ModalToken,
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
    /// Whether the modal sits in the overlay layer rather than inside its
    /// owner, so its route ends at the modal itself.
    overlay: bool,
}

/// Stack of admitted modals, included in structural rollback snapshots.
#[derive(Clone, Default)]
pub(super) struct ModalStack {
    /// Last scope is the sole normal input region.
    scopes: Vec<ModalScope>,
}

impl Core {
    /// Capture a live node's widget identity.
    fn modal_identity(&self, node: NodeId) -> Result<Identity> {
        self.validate_attached_node(node)?;
        Ok(Identity {
            node,
            incarnation: self.nodes[node].incarnation,
        })
    }

    /// Whether a saved identity still belongs to the active tree.
    fn modal_identity_live(&self, identity: Identity) -> bool {
        self.nodes
            .get(identity.node)
            .is_some_and(|entry| entry.incarnation == identity.incarnation)
            && self.is_attached_to_root(identity.node)
    }

    /// Whether `node` sits in the overlay layer: a child of the root.
    fn is_overlay(&self, node: NodeId) -> bool {
        self.nodes.get(node).and_then(|entry| entry.parent) == Some(self.root)
    }

    /// Whether a token still owns a scope, including a close awaiting
    /// completion.
    pub(crate) fn modal_is_open(&self, token: ModalToken) -> bool {
        self.modals.scopes.iter().any(|scope| scope.token == token)
    }

    /// Return the top normal-input subtree, if any.
    pub(crate) fn modal_region(&self) -> Option<NodeId> {
        self.modals.scopes.last().map(|scope| scope.options.modal)
    }

    /// Return the last ancestor eligible for modal binding routing.
    pub(crate) fn modal_owner(&self) -> Option<NodeId> {
        self.modals.scopes.last().map(|scope| scope.options.owner)
    }

    /// Whether normal input, focus, and capture may target this node.
    pub(crate) fn modal_admits(&self, node: NodeId) -> bool {
        self.modal_region()
            .is_none_or(|modal| self.is_ancestor_or_self(modal, node))
    }

    /// Return the transient mode that takes the next key.
    ///
    /// A framework-group modal suspends transient modes, so none is in effect
    /// while one is open. An application modal leaves the mode in effect.
    pub(crate) fn effective_transient_mode(&self) -> Option<&str> {
        if self.input_map.active_framework_group().is_some() {
            return None;
        }
        self.input_map.transient_mode()
    }

    /// Walk the input route from `start` toward the root.
    ///
    /// Each step yields a node and its path from the root. The walk ends after
    /// the modal owner, and it is empty when the modal does not admit `start`.
    /// Routing, analysis, and discovery share this walk, so they agree about
    /// which nodes a route reaches.
    pub(crate) fn route(&self, start: NodeId) -> Route<'_> {
        Route {
            core: self,
            next: self.modal_admits(start).then_some(start),
            path: self.path_of(self.root, start),
        }
    }

    /// Return the node the input route visits after `node`: its parent, or
    /// none at the modal owner.
    ///
    /// [`Core::route`] steps with this. A dispatch walk whose handlers can
    /// change the tree takes each step live, after the handler returns.
    pub(crate) fn route_step(&self, node: NodeId) -> Option<NodeId> {
        if self.modal_owner() == Some(node) {
            return None;
        }
        // An overlay modal is not inside its owner, so its route ends at the
        // modal rather than climbing through the root.
        if let Some(scope) = self.modals.scopes.last()
            && scope.overlay
            && scope.options.modal == node
        {
            return None;
        }
        self.nodes.get(node).and_then(|entry| entry.parent)
    }

    /// Return effects owned by scopes without altering widget-owned effects.
    pub(crate) fn modal_effects_for(&self, node: NodeId) -> Vec<Effect> {
        self.modals
            .scopes
            .iter()
            .filter(|scope| {
                scope.dim.is_some_and(|identity| {
                    identity.node == node && self.modal_identity_live(identity)
                })
            })
            .map(|_| effects::brightness(effects::MODAL_DIM))
            .collect()
    }

    /// Synchronize the binding map with the top scope after a stack change.
    pub(crate) fn sync_modal_bindings(&mut self) {
        self.input_map.set_modal_bindings(
            self.modals
                .scopes
                .last()
                .map(|scope| scope.options.bindings),
        );
    }

    /// Open one modal transaction, preserving prior interaction on failure.
    pub(crate) fn open_modal(&mut self, options: ModalOptions) -> Result<ModalToken> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = self.modal_identity(options.owner)?;
        let modal = self.modal_identity(options.modal)?;
        self.validate_attached_node(options.initial_focus)?;
        let dim = options
            .dim_target
            .map(|node| self.modal_identity(node))
            .transpose()?;
        // A modal in the overlay layer, a child of the root, may open over
        // its owner rather than inside it, and outside the current region:
        // only its owner must be admitted.
        let overlay = self.is_overlay(options.modal)
            && !self.is_ancestor_or_self(options.owner, options.modal);
        if !(overlay || self.is_ancestor_or_self(options.owner, options.modal))
            || !self.is_ancestor_or_self(options.modal, options.initial_focus)
            || !(overlay || self.modal_admits(options.modal))
            || !self.modal_admits(options.owner)
            || self
                .modals
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
            focus_ancestry.push(self.modal_identity(node)?);
            current = self.nodes[node].parent;
        }
        let token = ModalToken(
            NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| Error::Invalid("modal token space exhausted".into()))?,
        );
        let hidden = self.nodes[options.modal].hidden;
        let focus = self.focus;
        let capture = self.mouse_capture;
        self.nodes[options.modal].hidden = false;
        self.mouse_capture = None;
        self.modals.scopes.push(ModalScope {
            token,
            options,
            owner,
            modal,
            dim,
            focus_ancestry,
            overlay,
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
            self.modals.scopes.pop();
            self.sync_modal_bindings();
            self.nodes[options.modal].hidden = hidden;
            self.focus = focus;
            self.mouse_capture = capture;
            return Err(error);
        }
        self.invalidate(Invalidation::Layout);
        Ok(token)
    }

    /// Close the requested scope and all younger scopes after callbacks return.
    pub(crate) fn close_modal_now(&mut self, token: ModalToken) -> Result<()> {
        let Some(index) = self
            .modals
            .scopes
            .iter()
            .position(|scope| scope.token == token)
        else {
            return Ok(());
        };
        let retired: Vec<_> = self.modals.scopes.drain(index..).collect();
        self.sync_modal_bindings();
        self.mouse_capture = None;
        for scope in retired.iter().rev() {
            if self.modal_identity_live(scope.modal) {
                self.nodes[scope.modal.node].hidden = true;
            }
        }
        self.invalidate(Invalidation::Layout);
        let ancestry = &retired[0].focus_ancestry;
        let exact = ancestry
            .first()
            .filter(|identity| {
                self.modal_identity_live(**identity)
                    && self.modal_admits(identity.node)
                    && is_focus_candidate(self, identity.node, false)
            })
            .map(|identity| identity.node);
        let fallback = || {
            ancestry
                .iter()
                .filter(|identity| self.modal_identity_live(**identity))
                .find_map(|identity| {
                    self.subtree_pre_order(identity.node)
                        .into_iter()
                        .find(|node| {
                            self.modal_admits(*node) && is_focus_candidate(self, *node, false)
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
    pub(crate) fn retire_invalid_modals(&mut self) -> Result<()> {
        let invalid = self
            .modals
            .scopes
            .iter()
            .enumerate()
            .position(|(index, scope)| {
                !self.modal_identity_live(scope.owner)
                    || !self.modal_identity_live(scope.modal)
                    || !(scope.overlay
                        || self.is_ancestor_or_self(scope.options.owner, scope.options.modal))
                    || (index > 0
                        && !self.is_ancestor_or_self(
                            self.modals.scopes[index - 1].options.modal,
                            scope.options.owner,
                        ))
            });
        if let Some(index) = invalid {
            self.close_modal_now(self.modals.scopes[index].token)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    use super::*;
    use crate::{
        Context, ViewContext, Widget,
        input::{Event, key, mouse},
        testing::ttree::{Bb, TestTree, get_state, reset_state, run_ttree},
        widget::EventOutcome,
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

    /// Open `a` as a picker-like modal over the root, then `b`, a child of the
    /// root, as an overlay modal owned by `a`.
    fn open_overlay_over_a(core: &mut Core, tree: &TestTree) -> Result<(ModalToken, ModalToken)> {
        core.set_focus(tree.a_a)?;
        core.set_hidden(tree.b, true)?;
        let picker = core.open_modal(ModalOptions {
            owner: tree.root,
            modal: tree.a,
            initial_focus: tree.a_a,
            dim_target: None,
            bindings: ModalBindings::Application,
        })?;
        let question = core.open_modal(ModalOptions {
            owner: tree.a,
            modal: tree.b,
            initial_focus: tree.b_a,
            dim_target: Some(tree.a),
            bindings: ModalBindings::Application,
        })?;
        Ok((picker, question))
    }

    #[test]
    fn an_overlay_modal_opens_over_its_owner_and_takes_admission() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let core = &mut canopy.core;
            let (_, question) = open_overlay_over_a(core, &tree)?;
            // Admission moves to the overlay, and the owner's region is shut.
            assert_eq!(core.modal_region(), Some(tree.b));
            assert!(!core.nodes[tree.b].hidden, "opening shows the overlay");
            assert_eq!(core.focus, Some(tree.b_a));
            assert!(core.modal_admits(tree.b_a));
            assert!(!core.modal_admits(tree.a_a));
            assert!(core.set_focus(tree.a_a).is_err());
            // The route ends at the overlay rather than climbing to the root.
            let route: Vec<_> = core.route(tree.b_a).map(|(node, _)| node).collect();
            assert_eq!(route, [tree.b_a, tree.b]);
            // The owner's dialog dims behind the question.
            assert!(!core.modal_effects_for(tree.a).is_empty());
            // Closing restores focus inside the owner's region, and its dim.
            core.close_modal_after_dispatch(question)?;
            assert_eq!(core.modal_region(), Some(tree.a));
            assert_eq!(core.focus, Some(tree.a_a));
            assert!(core.nodes[tree.b].hidden, "closing hides the overlay");
            assert!(core.modal_effects_for(tree.a).is_empty());
            Ok(())
        })
    }

    #[test]
    fn closing_the_owner_closes_its_overlay_and_rollback_restores_both() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            let (picker, _) = open_overlay_over_a(&mut canopy.core, &tree)?;
            // A failed edit that closes nothing leaves the stack as it was.
            let failed = canopy.with_root_context(|context| {
                context.edit_structure(&mut |context| {
                    context.set_hidden(tree.a, true)?;
                    Err(Error::Invalid("abandoned".into()))
                })
            });
            assert!(failed.is_err());
            assert!(!canopy.core.nodes[tree.a].hidden, "the edit rolls back");
            assert_eq!(canopy.core.modal_region(), Some(tree.b));
            assert_eq!(canopy.core.focus, Some(tree.b_a));
            // Closing the older scope closes the overlay above it.
            canopy.core.close_modal_after_dispatch(picker)?;
            assert_eq!(canopy.core.modal_region(), None);
            assert!(canopy.core.nodes[tree.b].hidden, "the overlay closes");
            assert!(canopy.core.nodes[tree.a].hidden, "the picker closes");
            Ok(())
        })
    }

    #[test]
    fn removing_the_owner_retires_its_overlay() -> Result<()> {
        run_ttree(|canopy, _, tree| {
            open_overlay_over_a(&mut canopy.core, &tree)?;
            canopy.with_root_context(|context| context.remove_subtree(tree.a))?;
            assert_eq!(canopy.core.modal_region(), None, "both scopes retire");
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
            core.close_modal_after_dispatch(inner)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            assert_eq!(core.focus, Some(tree.b_a));
            let _inner = core.open_modal(ModalOptions {
                owner: tree.b,
                modal: tree.b_b,
                initial_focus: tree.b_b,
                dim_target: Some(tree.b_a),
                bindings: ModalBindings::Application,
            })?;
            core.close_modal_after_dispatch(outer)?;
            assert_eq!(core.modal_region(), None);
            assert_eq!(core.focus, Some(tree.a_a));
            assert_eq!(core.mouse_capture, None);
            assert!(core.nodes[tree.b].hidden);
            assert!(core.nodes[tree.b_b].hidden);
            assert!(core.modal_effects_for(tree.a).is_empty());
            assert_eq!(core.nodes[tree.a].effects.len(), 1);
            assert!(Arc::ptr_eq(&ordinary, &core.nodes[tree.a].effects[0]));
            core.close_modal_after_dispatch(outer)?;
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
            core.close_modal_after_dispatch(token)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            core.finish_dispatch(checkpoint, false)?;
            assert_eq!(core.modal_region(), Some(tree.b));
            let checkpoint = core.begin_dispatch();
            core.close_modal_after_dispatch(token)?;
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
            core.close_modal_after_dispatch(token)?;
            assert_eq!(core.focus, Some(tree.a));
            Ok(())
        })
    }

    /// Records the paste events it declines.
    struct PasteLog {
        name: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Widget for PasteLog {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            if matches!(event, Event::Paste(_)) {
                self.log.borrow_mut().push(self.name);
            }
            Ok(EventOutcome::Ignore)
        }
    }

    #[test]
    fn focus_events_stop_at_the_modal_owner() -> Result<()> {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut core = Core::new();
        let mut parent = core.root;
        let mut nodes = Vec::new();
        for name in ["outside", "owner", "modal", "leaf"] {
            let node = core.create_detached(PasteLog {
                name,
                log: Rc::clone(&log),
            })?;
            core.attach(parent, node)?;
            nodes.push(node);
            parent = node;
        }
        core.open_modal(ModalOptions {
            owner: nodes[1],
            modal: nodes[2],
            initial_focus: nodes[3],
            dim_target: None,
            bindings: ModalBindings::Application,
        })?;
        core.dispatch_event(nodes[3], &Event::Paste("text".into()))?;
        assert_eq!(*log.borrow(), ["leaf", "modal", "owner"]);
        Ok(())
    }
}
