use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::Layout,
    render::Render,
    widget::Widget,
    widgets::{Root, Text, frame},
};

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
        let text_id = c.add_orphan(Text::new(self.contents.clone()));
        let frame_id = frame::Frame::wrap(c, text_id)?;
        c.mount_child(frame_id)?;

        c.with_layout(&mut |layout| {
            *layout = Layout::fill();
        })?;
        Ok(())
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }
}

impl Loader for Pager {
    fn load(c: &mut Canopy) {
        c.add_commands::<Text>();
    }
}

/// Install key bindings for the pager demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .with_path("pager")
        .key_command('g', Text::cmd_scroll_to_top())
        .key_command('j', Text::cmd_scroll_down())
        .key_command(key::KeyCode::Down, Text::cmd_scroll_down())
        .mouse_command(mouse::Action::ScrollDown, Text::cmd_scroll_down())
        .key_command('k', Text::cmd_scroll_up())
        .key_command(key::KeyCode::Up, Text::cmd_scroll_up())
        .mouse_command(mouse::Action::ScrollUp, Text::cmd_scroll_up())
        .key_command('h', Text::cmd_scroll_left())
        .key_command(key::KeyCode::Left, Text::cmd_scroll_left())
        .key_command('l', Text::cmd_scroll_right())
        .key_command(key::KeyCode::Right, Text::cmd_scroll_right())
        .key_command(key::KeyCode::PageDown, Text::cmd_page_down())
        .key_command(' ', Text::cmd_page_down())
        .key_command(key::KeyCode::PageUp, Text::cmd_page_up())
        .with_path("root")
        .key_command('q', Root::cmd_quit());
}
