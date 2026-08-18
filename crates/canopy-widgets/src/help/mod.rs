//! Contextual key-binding help modal.

mod binding_list;
mod panel;
#[cfg(test)]
mod tests;

pub use binding_list::BindingList;
use canopy::{
    Canopy, ChildKey, Context, EventOutcome, Loader, NodeId, TypedId, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    event::Event,
    layout::{Align, Direction, Edges, Layout},
    render::Render,
    state::NodeName,
};
pub use panel::{ControlFooter, HelpPanel};

use crate::{frame::Frame, modal::Modal};

canopy::key!(pub(crate) ModalSlot: Modal);
canopy::key!(pub(crate) FrameSlot: Frame);
canopy::key!(pub(crate) PanelSlot: HelpPanel);
canopy::key!(pub(crate) BindingListSlot: BindingList);
canopy::key!(pub(crate) FooterSlot: ControlFooter);

/// Opaque overlay that owns the help modal subtree.
pub struct Help;

#[derive_commands]
impl Help {
    /// Build the complete help subtree and return its root.
    pub fn install(context: &mut dyn Context) -> Result<NodeId> {
        let bindings = context.create_detached(BindingList::new())?;
        let footer = context.create_detached(ControlFooter::new())?;

        let panel = context.create_detached(HelpPanel::new())?;
        context.attach_keyed(panel.into(), BindingListSlot::KEY, bindings.into())?;
        context.attach_keyed(panel.into(), FooterSlot::KEY, footer.into())?;

        let frame = context.create_detached(Frame::new().with_title("Key bindings"))?;
        context.attach_keyed(frame.into(), PanelSlot::KEY, panel.into())?;
        context.with_layout_of(frame.into(), &mut |layout| {
            *layout = Layout::fill()
                .max_width(72)
                .max_height(28)
                .padding(Edges::all(1));
        })?;

        let modal = context.create_detached(Modal::new())?;
        context.attach_keyed(modal.into(), FrameSlot::KEY, frame.into())?;
        context.with_layout_of(modal.into(), &mut |layout| {
            *layout = Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::Center)
                .align_vertical(Align::Center)
                .padding(Edges::all(1));
        })?;

        let help = context.create_detached(Self)?;
        context.attach_keyed(help.into(), ModalSlot::KEY, modal.into())?;
        context.set_layout_of(help, Layout::fill())?;
        Ok(help.into())
    }

    /// Return the binding-list node within an installed help subtree.
    pub(crate) fn binding_list_id(
        context: &dyn Context,
        help: NodeId,
    ) -> Result<TypedId<BindingList>> {
        let panel = Self::panel_id(context, help)?;
        context
            .get_child_in::<BindingListSlot>(panel)?
            .ok_or_else(|| Error::NotFound("binding_list".to_string()))
    }

    /// Return the panel node within an installed help subtree.
    fn panel_id(context: &dyn Context, help: NodeId) -> Result<NodeId> {
        let modal = context
            .get_child_in::<ModalSlot>(help)?
            .map(NodeId::from)
            .ok_or_else(|| Error::NotFound("modal".to_string()))?;
        let frame = context
            .get_child_in::<FrameSlot>(modal)?
            .map(NodeId::from)
            .ok_or_else(|| Error::NotFound("frame".to_string()))?;
        context
            .get_child_in::<PanelSlot>(frame)?
            .map(NodeId::from)
            .ok_or_else(|| Error::NotFound("help_panel".to_string()))
    }
}

impl Widget for Help {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.push_layer("help");
        render.fill("overlay", context.view().outer_rect_local(), ' ')?;
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Mouse(_)) {
            Ok(EventOutcome::Consume)
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
