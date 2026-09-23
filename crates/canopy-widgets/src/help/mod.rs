//! Contextual key-binding help modal.

mod binding_list;
mod mode;
#[cfg(test)]
mod tests;

pub use binding_list::BindingList;
use canopy::{
    Canopy, ChildSlot, Context, ContextExt, EventOutcome, Loader, NodeId, NodeName, Render,
    TypedId, ViewContext, Widget, derive_commands,
    error::{Error, Result},
    event::Event,
    layout::{Align, Direction, Edges, Layout, Sizing},
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
        context.with_layout_of(frame.into(), &mut |layout| {
            // The frame hugs its rows, so the list scrolls only once the
            // content outgrows the room the overlay leaves.
            *layout = Layout::fill()
                .height(Sizing::Measure)
                .max_width(72)
                .padding(Edges::all(1));
        })?;

        let modal = context.create_detached(Center::new())?;
        context.attach_slot(modal.into(), FrameSlot::KEY, frame.into())?;
        context.with_layout_of(modal.into(), &mut |layout| {
            // Two rows above and below keep the frame clear of the screen
            // edges, so a tall modal never fills the window.
            *layout = Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::Center)
                .align_vertical(Align::Center)
                .padding(Edges::symmetric(2, 1));
        })?;

        let help = context.create_detached(Self)?;
        context.attach_slot(help.into(), ModalSlot::KEY, modal.into())?;
        Ok(help.into())
    }

    /// Return the binding-list node within an installed help subtree.
    pub(crate) fn binding_list_id(
        context: &dyn Context,
        help: NodeId,
    ) -> Result<TypedId<BindingList>> {
        let modal = context
            .get_slot_of::<ModalSlot>(help)?
            .map(NodeId::from)
            .ok_or_else(|| Error::NotFound("modal".to_string()))?;
        let frame = context
            .get_slot_of::<FrameSlot>(modal)?
            .map(NodeId::from)
            .ok_or_else(|| Error::NotFound("frame".to_string()))?;
        context
            .get_slot_of::<BindingListSlot>(frame)?
            .ok_or_else(|| Error::NotFound("binding_list".to_string()))
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

impl Loader for Help {
    fn load(canopy: &mut Canopy) -> Result<()> {
        canopy.add_commands::<Self>()?;
        BindingList::load(canopy)?;
        Ok(())
    }
}
