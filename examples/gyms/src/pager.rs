use canopy::{
    CanopyBuilder, Context, ContextExt, Register, Setup, Widget, error::Result, layout::Layout,
};
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
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_child(c.node_id(), Frame::new())?;
        c.add_child(
            frame_id,
            Text::new(self.contents.clone()).with_focusable(true),
        )?;

        c.set_layout_override(c.node_id(), Layout::fill().into())?;
        Ok(())
    }
}

impl Register for Pager {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Text>()?;
        Ok(())
    }
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.script("pager", crate::TEXT_SCROLL_BINDINGS)
}
