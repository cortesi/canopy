use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::{Layout, Sizing},
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

    /// Ensure the frame and text subtree is built.
    fn ensure_tree(&self, c: &mut dyn Context) {
        if !c.children().is_empty() {
            return;
        }

        let frame_id = c
            .add_child(frame::Frame::new())
            .expect("Failed to mount frame");
        let text_id = c
            .add_child_to(frame_id, Text::new(self.contents.clone()))
            .expect("Failed to mount text");

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })
        .expect("Failed to configure layout");
        c.with_layout_of(frame_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
        .expect("Failed to configure frame layout");
        c.with_layout_of(text_id, &mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to configure text layout");
    }
}

impl Widget for Pager {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c);
        None
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
