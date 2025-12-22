use std::time::Duration;

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::Event,
    geom::Rect,
    render::Render,
    widget::{EventOutcome, Widget},
    widgets::Text,
};
use taffy::style::{Dimension, Display, FlexDirection, Style};

/// Demo node that displays placeholder text.
pub struct TextDisplay {
    /// Text content for the demo.
    paragraph: String,
    /// Text node id.
    text_id: Option<NodeId>,
}

impl Default for TextDisplay {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl TextDisplay {
    /// Construct a new text display demo.
    pub fn new() -> Self {
        let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.\
                        Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.\
                        Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod \
                        tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                        quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo \
                        consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse \
                        cillum dolore eu fugiat nulla pariatur.
                        ";

        Self {
            paragraph: paragraph.to_string(),
            text_id: None,
        }
    }

    #[command]
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}

    /// Ensure the text node is created and attached.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.text_id.is_some() {
            return;
        }

        let text_id = c.add(Box::new(Text::new(self.paragraph.clone())));
        c.set_children(c.node_id(), vec![text_id])
            .expect("Failed to attach text");

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)
            .expect("Failed to style root");

        let mut grow = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(text_id, &mut grow)
            .expect("Failed to style text");

        self.text_id = Some(text_id);
    }
}

impl Widget for TextDisplay {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
    }
}

impl Loader for TextDisplay {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
    }
}
