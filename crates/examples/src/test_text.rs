use std::time::Duration;

use canopy::{
    Canopy, Context, Loader, ViewContext, command, derive_commands, error::Result, geom::Rect,
    layout::Dimension, render::Render, widget::Widget, widgets::Text,
};

/// Demo node that displays placeholder text.
pub struct TextDisplay {
    /// Text content for the demo.
    paragraph: String,
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
        }
    }

    #[command]
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}

    /// Ensure the text node is created and attached.
    fn ensure_tree(&self, c: &mut dyn Context) {
        if !c.children().is_empty() {
            return;
        }

        let text_id = c
            .add_child(Text::new(self.paragraph.clone()))
            .expect("Failed to mount text");

        c.with_layout(&mut |layout| {
            layout.flex_col();
        })
        .expect("Failed to configure layout");
        c.with_layout_of(text_id, &mut |layout| {
            layout.flex_item(1.0, 1.0, Dimension::Auto);
        })
        .expect("Failed to configure text layout");
    }
}

impl Widget for TextDisplay {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
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
