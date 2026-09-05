use canopy::{CanopyBuilder, prelude::*};
use canopy_widgets::{Frame, Text};

/// Simple pager widget for file contents.
pub struct Pager {
    /// Contents to display.
    contents: String,
}

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
}

impl Loader for Pager {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Text>()?;
        Ok(())
    }
}

/// Install key bindings for the pager demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(&crate::text_scroll_bindings("text", "pager"))?;
    Ok(())
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("pager", crate::text_scroll_bindings("text", "pager"))
}
