use canopy::{derive_commands, prelude::*};
use canopy_widgets::{Frame, Text};

/// Default bindings for the pager demo.
const DEFAULT_BINDINGS: &str = r#"
canopy.bind("g", { path = "pager", description = "Top" }, function()
    text.scroll_to(0, 0)
end)
canopy.bind("j", { path = "pager", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind("Down", { path = "pager", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind_mouse("ScrollDown", { path = "pager", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind("k", { path = "pager", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind("Up", { path = "pager", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind_mouse("ScrollUp", { path = "pager", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind("h", { path = "pager", description = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind("Left", { path = "pager", description = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind("l", { path = "pager", description = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind("Right", { path = "pager", description = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind("PageDown", { path = "pager", description = "Page down" }, function()
    text.page(1)
end)
canopy.bind("Space", { path = "pager", description = "Page down" }, function()
    text.page(1)
end)
canopy.bind("PageUp", { path = "pager", description = "Page up" }, function()
    text.page(-1)
end)
canopy.bind("q", { path = "root", description = "Quit" }, function()
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
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_child(Frame::new())?;
        c.add_child_to(frame_id, Text::new(self.contents.clone()))?;

        c.set_layout(Layout::fill())?;
        Ok(())
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    Ok(())
}
