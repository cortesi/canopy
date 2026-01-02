use canopy::{
    Canopy, Context, Loader, ReadContext, Widget, command, derive_commands,
    error::Result,
    layout::{Layout, Sizing},
    render::Render,
};
use canopy_widgets::Text;

/// Placeholder paragraph text for the demo.
const LOREM: &str = concat!(
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ",
    "ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ",
    "ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in ",
    "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. ",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ",
    "ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ",
    "ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in ",
    "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. ",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ",
    "ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ",
    "ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in ",
    "reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.",
);

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
        Self {
            paragraph: LOREM.to_string(),
        }
    }

    #[command]
    /// Trigger a redraw.
    pub fn redraw(&mut self, _ctx: &mut dyn Context) {}
}

impl Widget for TextDisplay {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let text_id = c.add_child(Text::new(self.paragraph.clone()))?;

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        c.with_layout_of(text_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

impl Loader for TextDisplay {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
    }
}
