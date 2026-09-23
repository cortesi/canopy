//! Contextual key-binding help modal.

mod binding_list;
mod mode;
#[cfg(test)]
mod tests;

pub use binding_list::BindingList;
use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    ViewContextExt, Widget, derive_commands,
    error::{Error, Result},
    input::Event,
    layout::{Edges, Layout},
    render::Render,
};
pub use mode::ModeHelp;

use crate::Dialog;

/// Transparent overlay that owns the help modal subtree.
///
/// The overlay draws nothing of its own, so the dimmed application stays
/// visible around the help panel. It still consumes mouse input.
pub struct Help;

#[derive_commands]
impl Help {
    /// Build the complete help subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let help = context.create_detached(Self)?;
        // Two rows above and below keep the frame clear of the screen edges,
        // so a tall modal never fills the window, and the list scrolls only
        // once the content outgrows the room that leaves.
        Dialog::new()
            .with_title("Keyboard shortcuts")
            .with_max_width(72)
            .with_margin(Edges::symmetric(2, 1))
            .add(context, help.into(), BindingList::new())?;
        Ok(help.into())
    }

    /// Return the binding-list node within an installed help subtree.
    pub(crate) fn binding_list_id(
        context: &dyn Context,
        help: NodeId,
    ) -> Result<TypedId<BindingList>> {
        context
            .unique_descendant::<BindingList>(help)?
            .ok_or_else(|| Error::NotFound("help binding list".into()))
    }
}

impl Widget for Help {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("help");
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Mouse(_)) {
            Ok(EventOutcome::Handle)
        } else {
            Ok(EventOutcome::Ignore)
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("help")
    }
}

impl Register for Help {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        BindingList::register(setup)?;
        Ok(())
    }
}
