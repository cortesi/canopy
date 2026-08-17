use super::*;
use crate::{event::Event, widget::EventOutcome};

impl Core {
    /// Build a command-scope frame for a specific event.
    pub(crate) fn command_scope_for_event(&self, event: &Event) -> CommandScopeFrame {
        let mut frame = self.current_command_scope().cloned().unwrap_or_default();
        frame.event = Some(event.clone());
        frame.mouse = match event {
            Event::Mouse(mouse) => Some(*mouse),
            _ => None,
        };
        frame
    }

    /// Dispatch an event to a node, bubbling to parents if unhandled.
    pub fn dispatch_event(
        &mut self,
        start: impl Into<NodeId>,
        event: &Event,
    ) -> Result<EventOutcome> {
        let start = start.into();
        let depth = self.push_command_scope(self.command_scope_for_event(event));
        let outcome = self.dispatch_event_inner(start, event);
        self.pop_command_scope(depth);
        outcome
    }

    /// Dispatch an event to a node and bubble until handled.
    fn dispatch_event_inner(&mut self, start: NodeId, event: &Event) -> Result<EventOutcome> {
        let mut target = Some(start);
        while let Some(id) = target {
            let outcome = self.with_widget_ctx(id, |w, ctx| w.on_event(event, ctx))??;
            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => return Ok(outcome),
                EventOutcome::Ignore => {
                    target = self.nodes[id].parent;
                }
            }
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
        let depth = self.push_command_scope(self.command_scope_for_event(event));
        let outcome = self.with_widget_ctx(node_id, |w, ctx| w.on_event(event, ctx));
        self.pop_command_scope(depth);
        let outcome = outcome??;
        Ok(outcome)
    }
}
