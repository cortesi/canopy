use super::*;
use crate::widget::EventOutcome;

impl Core {
    /// Dispatch an event along the input route from `start` until a widget
    /// handles it.
    ///
    /// The route has the modal bounds of [`Core::route`].
    pub fn dispatch_event(
        &mut self,
        start: impl Into<NodeId>,
        event: &Event,
    ) -> Result<EventOutcome> {
        let start = start.into();
        let depth = self.push_event_scope(event);
        let outcome = self.dispatch_event_inner(start, event);
        self.pop_event_scope(depth);
        outcome
    }

    /// Dispatch an event along the route until a widget handles it.
    ///
    /// Handlers can change the tree, so each step reads the next node only
    /// after the handler returns.
    fn dispatch_event_inner(&mut self, start: NodeId, event: &Event) -> Result<EventOutcome> {
        let mut target = self.interaction_admits(start).then_some(start);
        while let Some(id) = target {
            let outcome = self.with_widget_ctx(id, |w, ctx| w.on_event(event, ctx))??;
            if outcome == EventOutcome::Handle {
                return Ok(outcome);
            }
            target = self.route_step(id);
        }
        Ok(EventOutcome::Ignore)
    }

    /// Dispatch an event to a single node without bubbling.
    pub fn dispatch_event_on_node(
        &mut self,
        node_id: impl Into<NodeId>,
        event: &Event,
    ) -> Result<EventOutcome> {
        let node_id = node_id.into();
        let depth = self.push_event_scope(event);
        let outcome = self.with_widget_ctx(node_id, |w, ctx| w.on_event(event, ctx));
        self.pop_event_scope(depth);
        let outcome = outcome??;
        Ok(outcome)
    }
}
