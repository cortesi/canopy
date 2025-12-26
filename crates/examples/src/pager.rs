use std::time::Duration;

use canopy::{
    Canopy, Context, Loader, ViewContext, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::Rect,
    layout::Dimension,
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

        c.build().flex_col();
        c.build_node(frame_id).flex_item(1.0, 1.0, Dimension::Auto);
        c.build_node(text_id).flex_item(1.0, 1.0, Dimension::Auto);
    }
}

impl Widget for Pager {
    fn accept_focus(&self) -> bool {
        true
    }

    fn render(&mut self, _rndr: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
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
    cnpy.bind_key_command('g', "pager", Text::cmd_scroll_to_top())
        .unwrap();

    cnpy.bind_key_command('j', "pager", Text::cmd_scroll_down())
        .unwrap();
    cnpy.bind_key_command(key::KeyCode::Down, "pager", Text::cmd_scroll_down())
        .unwrap();
    cnpy.bind_mouse_command(mouse::Action::ScrollDown, "pager", Text::cmd_scroll_down())
        .unwrap();
    cnpy.bind_key_command('k', "pager", Text::cmd_scroll_up())
        .unwrap();
    cnpy.bind_key_command(key::KeyCode::Up, "pager", Text::cmd_scroll_up())
        .unwrap();
    cnpy.bind_mouse_command(mouse::Action::ScrollUp, "pager", Text::cmd_scroll_up())
        .unwrap();

    cnpy.bind_key_command('h', "pager", Text::cmd_scroll_left())
        .unwrap();
    cnpy.bind_key_command(key::KeyCode::Left, "pager", Text::cmd_scroll_left())
        .unwrap();
    cnpy.bind_key_command('l', "pager", Text::cmd_scroll_right())
        .unwrap();
    cnpy.bind_key_command(key::KeyCode::Right, "pager", Text::cmd_scroll_right())
        .unwrap();

    cnpy.bind_key_command(key::KeyCode::PageDown, "pager", Text::cmd_page_down())
        .unwrap();
    cnpy.bind_key_command(' ', "pager", Text::cmd_page_down())
        .unwrap();
    cnpy.bind_key_command(key::KeyCode::PageUp, "pager", Text::cmd_page_up())
        .unwrap();

    cnpy.bind_key_command('q', "root", Root::cmd_quit())
        .unwrap();
}
