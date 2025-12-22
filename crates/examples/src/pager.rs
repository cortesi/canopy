use std::time::Duration;

use canopy::{
    Canopy, Context, Loader, NodeId, ViewContext, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::Rect,
    render::Render,
    widget::{EventOutcome, Widget},
    widgets::{Text, frame},
};
use taffy::style::{Dimension, Display, FlexDirection, Style};

/// Simple pager widget for file contents.
pub struct Pager {
    /// Contents to display.
    contents: String,
    /// Frame node id.
    frame_id: Option<NodeId>,
}

#[derive_commands]
impl Pager {
    /// Construct a pager with initial contents.
    pub fn new(contents: &str) -> Self {
        Self {
            contents: contents.to_string(),
            frame_id: None,
        }
    }

    /// Ensure the frame and text subtree is built.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.frame_id.is_some() {
            return;
        }

        let text_id = c.add(Box::new(Text::new(self.contents.clone())));
        let frame_id = c.add(Box::new(frame::Frame::new()));
        c.mount_child(frame_id, text_id)
            .expect("Failed to mount text");
        c.set_children(c.node_id(), vec![frame_id])
            .expect("Failed to attach frame");

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
        c.with_style(frame_id, &mut grow)
            .expect("Failed to style frame");
        c.with_style(text_id, &mut grow)
            .expect("Failed to style text");

        self.frame_id = Some(frame_id);
    }
}

impl Widget for Pager {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _rndr: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
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

impl Loader for Pager {
    fn load(c: &mut Canopy) {
        c.add_commands::<Text>();
    }
}

/// Install key bindings for the pager demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.bind_key('g', "pager", "text::scroll_to_top()")
        .unwrap();

    cnpy.bind_key('j', "pager", "text::scroll_down()").unwrap();
    cnpy.bind_key(key::KeyCode::Down, "pager", "text::scroll_down()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollDown, "pager", "text::scroll_down()")
        .unwrap();
    cnpy.bind_key('k', "pager", "text::scroll_up()").unwrap();
    cnpy.bind_key(key::KeyCode::Up, "pager", "text::scroll_up()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollUp, "pager", "text::scroll_up()")
        .unwrap();

    cnpy.bind_key('h', "pager", "text::scroll_left()").unwrap();
    cnpy.bind_key(key::KeyCode::Left, "pager", "text::scroll_left()")
        .unwrap();
    cnpy.bind_key('l', "pager", "text::scroll_right()").unwrap();
    cnpy.bind_key(key::KeyCode::Right, "pager", "text::scroll_right()")
        .unwrap();

    cnpy.bind_key(key::KeyCode::PageDown, "pager", "text::page_down()")
        .unwrap();
    cnpy.bind_key(' ', "pager", "text::page_down()").unwrap();
    cnpy.bind_key(key::KeyCode::PageUp, "pager", "text::page_up()")
        .unwrap();

    cnpy.bind_key('q', "root", "root::quit()").unwrap();
}
