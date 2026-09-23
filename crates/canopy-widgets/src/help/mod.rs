//! Contextual key-binding help modal.

mod binding_list;
mod mode;
#[cfg(test)]
mod tests;

pub use binding_list::BindingList;
use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, Register, Setup, TypedId, ViewContext,
    Widget, derive_commands,
    error::Result,
    input::Event,
    layout::{Align, Direction, Edges, Layout, Sizing},
    render::Render,
    tree::ChildSlot,
};
pub use mode::ModeHelp;

use crate::{center::Center, frame::Frame};

canopy::slot!(pub(crate) ModalSlot: Center);
canopy::slot!(pub(crate) FrameSlot: Frame);
canopy::slot!(pub(crate) BindingListSlot: BindingList);

/// Transparent overlay that owns the help modal subtree.
///
/// The overlay draws nothing of its own, so the dimmed application stays
/// visible around the help panel. It still consumes mouse input.
pub struct Help;

#[derive_commands]
impl Help {
    /// Build the complete help subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let bindings = context.create_detached(BindingList::new())?;

        let frame = context.create_detached(Frame::new().with_title("Keyboard shortcuts"))?;
        context.attach_slot(frame.into(), BindingListSlot::KEY, bindings.into())?;
        // The frame hugs its rows, so the list scrolls only once the content
        // outgrows the room the overlay leaves.
        context.set_layout_override(
            frame.into(),
            Layout::fill()
                .height(Sizing::Measure)
                .max_width(72)
                .padding(Edges::all(1))
                .into(),
        )?;

        let modal = context.create_detached(Center::new())?;
        context.attach_slot(modal.into(), FrameSlot::KEY, frame.into())?;
        // Two rows above and below keep the frame clear of the screen edges,
        // so a tall modal never fills the window.
        context.set_layout_override(
            modal.into(),
            Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::Center)
                .align_vertical(Align::Center)
                .padding(Edges::symmetric(2, 1))
                .into(),
        )?;

        let help = context.create_detached(Self)?;
        context.attach_slot(help.into(), ModalSlot::KEY, modal.into())?;
        Ok(help.into())
    }

    /// Return the binding-list node within an installed help subtree.
    pub(crate) fn binding_list_id(
        context: &dyn Context,
        help: NodeId,
    ) -> Result<TypedId<BindingList>> {
        let modal = context.get_slot::<ModalSlot>(help)?;
        let frame = context.get_slot::<FrameSlot>(modal)?;
        context.get_slot::<BindingListSlot>(frame)
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
