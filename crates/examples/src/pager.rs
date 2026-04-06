use canopy::{derive_commands, prelude::*};
use canopy_widgets::{Frame, Text};

/// Default bindings for the pager demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind_with("g", { path = "pager", desc = "Top" }, function()
    text.scroll_to(0, 0)
end)
canopy.bind_with("j", { path = "pager", desc = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind_with("Down", { path = "pager", desc = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind_mouse_with("ScrollDown", { path = "pager", desc = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind_with("k", { path = "pager", desc = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind_with("Up", { path = "pager", desc = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind_mouse_with("ScrollUp", { path = "pager", desc = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind_with("h", { path = "pager", desc = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind_with("Left", { path = "pager", desc = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind_with("l", { path = "pager", desc = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind_with("Right", { path = "pager", desc = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind_with("PageDown", { path = "pager", desc = "Page down" }, function()
    text.page(1)
end)
canopy.bind_with("Space", { path = "pager", desc = "Page down" }, function()
    text.page(1)
end)
canopy.bind_with("PageUp", { path = "pager", desc = "Page up" }, function()
    text.page(-1)
end)
canopy.bind_with("q", { path = "root", desc = "Quit" }, function()
    root.quit()
end)
"#;

/// Simple pager widget for file contents.
pub struct Pager {
    /// Contents to display.
    contents: String,
}

#[derive_commands]
impl Pager {
    /// Construct a pager with initial contents.
    pub fn new(contents: &str) -> Self {
        Self {
            contents: contents.to_string(),
        }
    }
}

impl Widget for Pager {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_child(Frame::new())?;
        c.add_child_to(frame_id, Text::new(self.contents.clone()))?;

        c.set_layout(Layout::fill())?;
        Ok(())
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

impl Loader for Pager {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Text>()?;
        Ok(())
    }
}

/// Install key bindings for the pager demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.run_default_script(DEFAULT_BINDINGS).unwrap();
}
