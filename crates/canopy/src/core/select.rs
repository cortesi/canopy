//! Consumer-aware binding selection shared by routing and analysis.
//!
//! The registry ranks candidates. An intent candidate is eligible only
//! when the node's widget accepts the action in its current state. Routing,
//! route explanation, help, checked dispatch, and diagnostics all select
//! through this module, so they cannot disagree about the winner.

use crate::{
    core::{
        Core, NodeId,
        context::CoreViewContext,
        inputmap::{BindingId, BindingRecord, InputSpec},
        world::WidgetOperation,
    },
    error::Result,
    event::{Event, key::Key},
    path::Path,
    widget::EventOutcome,
};

impl Core {
    /// Select the first eligible binding for `key` at `node`.
    ///
    /// An action candidate is eligible only when the node's widget accepts the
    /// action. `focus` is the route start, so acceptance answers for the route
    /// the caller inspects. `excluded` names action bindings already attempted
    /// at this node after a release mismatch.
    pub(crate) fn select_key_binding(
        &self,
        node: NodeId,
        path: &Path,
        key: Key,
        focus: NodeId,
        excluded: &[BindingId],
    ) -> Option<&BindingRecord> {
        self.input_map
            .candidates(path, InputSpec::Key(key))
            .into_iter()
            .map(|candidate| candidate.record)
            .find(|record| {
                !excluded.contains(&record.id)
                    && record
                        .action
                        .intent()
                        .is_none_or(|action| self.node_accepts_intent(node, action.as_str(), focus))
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
        let depth = self.push_event_scope(event);
        let outcome =
            self.with_widget_ctx(node, |widget, context| widget.on_intent(action, context));
        self.pop_event_scope(depth);
        let outcome = outcome??;
        Ok(outcome)
    }
}
