//! Consumer-aware binding selection shared by routing and analysis.
//!
//! The registry ranks candidates. An intent candidate is eligible only
//! when the node's widget accepts the action in its current state, and a
//! command candidate only when its command has a target that shows. Routing,
//! route explanation, help, checked dispatch, and diagnostics all select
//! through this module, so they cannot disagree about the winner.

use crate::{
    NodeId,
    commands::{CommandCall, CommandResolver},
    core::{Core, context::CoreViewContext, inputmap::BindingRecord, world::WidgetOperation},
    error::Result,
    input::{BindingAction, BindingId, Event, InputSpec, NavIntent, key::Key},
    path::Path,
    widget::EventOutcome,
};

impl Core {
    /// Select the first eligible binding for `key` at `node`.
    pub(crate) fn select_key_binding(
        &self,
        node: NodeId,
        path: &Path,
        key: Key,
        focus: NodeId,
        excluded: &[BindingId],
    ) -> Option<&BindingRecord> {
        self.select_binding(node, path, InputSpec::Key(key), focus, excluded)
    }

    /// Select the first eligible binding for `input` at `node`.
    ///
    /// An intent candidate is eligible only when the node's widget accepts the
    /// intent, and a command candidate only when its command has a target that
    /// shows. An ineligible candidate is dormant: it falls through to the next
    /// candidate, so it never shadows a usable binding, and help never lists
    /// it. `focus` is the route start, so acceptance answers for the route the
    /// caller inspects. `excluded` names action bindings already attempted at
    /// this node after a release mismatch.
    pub(crate) fn select_binding(
        &self,
        node: NodeId,
        path: &Path,
        input: InputSpec,
        focus: NodeId,
        excluded: &[BindingId],
    ) -> Option<&BindingRecord> {
        self.input_map
            .candidates(path, input)
            .into_iter()
            .map(|candidate| candidate.record)
            .find(|record| {
                !excluded.contains(&record.id) && self.binding_eligible(record, node, focus)
            })
    }

    /// Return whether a candidate bound at `node` can act now.
    fn binding_eligible(&self, record: &BindingRecord, node: NodeId, focus: NodeId) -> bool {
        match &record.action {
            BindingAction::Intent(action) => self.node_accepts_intent(node, action.as_str(), focus),
            BindingAction::Command(call) => self.command_has_target(call, node),
            BindingAction::Script(_) | BindingAction::Menu(_) => true,
        }
    }

    /// Return whether `call`, bound at `node`, resolves to a target that
    /// shows.
    ///
    /// A command the registry does not know stays eligible, so dispatch
    /// reports the mistake instead of the key falling through in silence.
    fn command_has_target(&self, call: &CommandCall, node: NodeId) -> bool {
        self.commands.get(call.id.0).is_none_or(|spec| {
            CommandResolver::for_target(self, call.target_or(node))
                .resolve(spec)
                .is_some()
        })
    }

    /// Select the binding a transient mode runs for `key` on `start`'s route.
    ///
    /// The winner is the first node on the modal-bounded route with an
    /// eligible binding. Routing and analysis both use this, so a transient
    /// key dispatches where its explanation says it will.
    pub(crate) fn transient_winner(
        &self,
        start: NodeId,
        key: Key,
    ) -> Option<(NodeId, Path, &BindingRecord)> {
        self.route(start).find_map(|(node, path)| {
            self.select_key_binding(node, &path, key, start, &[])
                .map(|binding| (node, path, binding))
        })
    }

    /// Return the widget's key prediction at `node` on `focus`'s route.
    ///
    /// Fails when the widget cannot be read, as when it is running a
    /// callback.
    pub(crate) fn node_key_outcome(
        &self,
        node: NodeId,
        key: Key,
        focus: NodeId,
    ) -> Result<EventOutcome> {
        let context = CoreViewContext::with_focus(self, node, focus);
        self.with_widget(
            node,
            WidgetOperation::access("key prediction"),
            |widget, _| widget.key_outcome(key, &context),
        )
    }

    /// Return the widget's key prediction for analysis.
    ///
    /// A widget that cannot be read, because it is running the callback that
    /// asks, predicts [`EventOutcome::Ignore`], as it would decline an action.
    pub(crate) fn predict_key(&self, node: NodeId, key: Key, focus: NodeId) -> EventOutcome {
        self.node_key_outcome(node, key, focus)
            .unwrap_or(EventOutcome::Ignore)
    }

    /// Return whether the widget at `node` accepts `action` on `focus`'s
    /// route.
    ///
    /// A widget that cannot be read declines. Selection and dispatch share
    /// this test, so an unreadable widget never becomes an action consumer.
    pub(crate) fn node_accepts_intent(&self, node: NodeId, action: &str, focus: NodeId) -> bool {
        self.widget_accepts_intent(node, action, focus)
            || NavIntent::from_name(action).is_some_and(|nav| self.nav_scroll(node, nav).is_some())
    }

    /// Return whether the widget at `node` itself accepts `action`.
    fn widget_accepts_intent(&self, node: NodeId, action: &str, focus: NodeId) -> bool {
        let context = CoreViewContext::with_focus(self, node, focus);
        self.with_widget(
            node,
            WidgetOperation::access("action selection"),
            |widget, _| widget.accepts_intent(action, &context),
        )
        .unwrap_or(false)
    }

    /// Dispatch one intent to a node.
    pub(crate) fn dispatch_action_on_node(
        &mut self,
        node: NodeId,
        action: &str,
        event: &Event,
    ) -> Result<EventOutcome> {
        // A navigation intent the widget declines is the runtime's default:
        // it scrolls the node's view.
        let focus = self.focus.unwrap_or(self.root);
        if let Some(nav) = NavIntent::from_name(action)
            && !self.widget_accepts_intent(node, action, focus)
            && let Some(op) = self.nav_scroll(node, nav)
        {
            self.scroll(node, op);
            return Ok(EventOutcome::Handle);
        }
        let depth = self.push_event_scope(event);
        let outcome =
            self.with_widget_ctx(node, |widget, context| widget.on_intent(action, context));
        self.pop_event_scope(depth);
        let outcome = outcome??;
        Ok(outcome)
    }
}
