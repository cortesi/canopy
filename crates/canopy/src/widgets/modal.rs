//! Modal widget for centered overlay content.

use crate::{
    ViewContext, derive_commands,
    error::Result,
    layout::{Align, Direction, Layout},
    render::Render,
    state::NodeName,
    widget::Widget,
};

/// A modal container that centers its content.
///
/// For the dimming effect, the parent should push an effect on the background content
/// using `c.push_effect(background_id, effects::dim(0.5))`. The Modal itself renders
/// at full brightness since it's a sibling to the dimmed content, not a descendant.
///
/// # Example
///
/// ```ignore
/// // In your app widget with Stack layout
/// fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
///     let modal_id = c.add(Modal::new());
///     let frame_id = c.add(frame::Frame::new().with_title("Dialog"));
///     let content_id = c.add(my_content);
///
///     c.mount_child_to(frame_id, content_id)?;
///     c.mount_child_to(modal_id, frame_id)?;
///
///     // Dim the background content
///     c.push_effect(self.content_id, effects::dim(0.5))?;
///
///     // Update children to include modal (Stack layout makes it overlay)
///     self.modal_id = Some(modal_id);
///     self.sync_children(c)?;
///
///     Ok(())
/// }
/// ```
pub struct Modal;

#[derive_commands]
impl Modal {
    /// Create a new Modal widget.
    pub fn new() -> Self {
        Self
    }
}

impl Default for Modal {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Modal {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::Center)
            .align_vertical(Align::Center)
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("modal")
    }
}
