use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{Event, key},
    geom::{Expanse, Rect},
    render::Render,
    widget::{EventOutcome, Widget},
    widgets::{Root, frame},
};
use taffy::{
    geometry::Size,
    style::{AvailableSpace, Dimension, Display, FlexDirection, Style},
};

/// A widget that renders a test pattern.
pub struct TestPattern {
    /// Virtual canvas size.
    size: Expanse,
}

impl Default for TestPattern {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl TestPattern {
    /// Construct the test pattern node.
    pub fn new() -> Self {
        Self {
            size: Expanse::new(500, 500),
        }
    }

    #[command]
    /// Scroll to the top-left corner.
    pub fn scroll_to_top(&mut self, c: &mut dyn Context) {
        c.scroll_to(0, 0);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down();
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up();
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left();
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right();
    }

    #[command]
    /// Page down in the viewport.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down();
    }

    #[command]
    /// Page up in the viewport.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up();
    }

    /// Return the character for the test pattern at a position.
    fn generate_pattern_char(x: u32, y: u32) -> char {
        // Pattern: "abcdefghijklmnopqrstuvwxyz0123456789"
        let pattern = "abcdefghijklmnopqrstuvwxyz0123456789";
        let pattern_len = pattern.len() as u32;

        // Offset each row by one more character than the previous
        let index = ((x + y) % pattern_len) as usize;
        pattern.chars().nth(index).unwrap_or(' ')
    }
}

impl Widget for TestPattern {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();

        // The viewport automatically handles the visible window for us
        // We just need to render the content that's visible
        for y in 0..view.h {
            let absolute_y = view.tl.y + y;
            if absolute_y >= self.size.h {
                break;
            }

            let mut line = String::new();
            for x in 0..view.w {
                let absolute_x = view.tl.x + x;
                if absolute_x >= self.size.w {
                    break;
                }
                let ch = Self::generate_pattern_char(absolute_x, absolute_y);
                line.push(ch);
            }

            // Use different colors to make the pattern more visible
            let color = match (absolute_y / 10) % 3 {
                0 => "blue",
                1 => "green",
                _ => "yellow",
            };

            r.text(color, view.line(y), &line)?;
        }

        Ok(())
    }

    fn measure(
        &self,
        _known_dimensions: Size<Option<f32>>,
        _available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        Size {
            width: self.size.w as f32,
            height: self.size.h as f32,
        }
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }
}

/// Root node for the frame gym demo.
pub struct FrameGym {
    /// Frame node id.
    frame_id: Option<NodeId>,
}

impl Default for FrameGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl FrameGym {
    /// Construct a new frame gym.
    pub fn new() -> Self {
        Self { frame_id: None }
    }

    /// Ensure the frame and pattern nodes are built.
    fn ensure_tree(&mut self, c: &mut dyn Context) {
        if self.frame_id.is_some() {
            return;
        }

        let pattern_id = c.add(Box::new(TestPattern::new()));
        let frame_id = c.add(Box::new(frame::Frame::new().with_title("Frame Gym")));
        c.mount_child(frame_id, pattern_id)
            .expect("Failed to mount pattern");
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
        c.with_style(pattern_id, &mut grow)
            .expect("Failed to style pattern");

        self.frame_id = Some(frame_id);
    }
}

impl Widget for FrameGym {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
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

impl Loader for FrameGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<TestPattern>();
    }
}

/// Install key bindings for the frame gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("")
        // Focus navigation
        .key(key::KeyCode::Tab, "root::focus_next()")
        // Arrow keys for scrolling
        .key('g', "test_pattern::scroll_to_top()")
        .key(key::KeyCode::Down, "test_pattern::scroll_down()")
        .key(key::KeyCode::Up, "test_pattern::scroll_up()")
        .key(key::KeyCode::Left, "test_pattern::scroll_left()")
        .key(key::KeyCode::Right, "test_pattern::scroll_right()")
        // Vim-style navigation
        .key('j', "test_pattern::scroll_down()")
        .key('k', "test_pattern::scroll_up()")
        .key('h', "test_pattern::scroll_left()")
        .key('l', "test_pattern::scroll_right()")
        // Page navigation
        .key(key::KeyCode::PageDown, "test_pattern::page_down()")
        .key(' ', "test_pattern::page_down()")
        .key(key::KeyCode::PageUp, "test_pattern::page_up()")
        // Quit
        .with_path("root")
        .key('q', "root::quit()");
}

#[cfg(test)]
mod tests {
    use canopy::testing::harness::Harness;

    use super::*;

    #[test]
    fn test_framegym_basic() -> Result<()> {
        let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
        harness.render()?;

        // Debug: print all lines to see what's happening
        println!("\n=== Rendered output ===");
        for (i, line) in harness.tbuf().lines().iter().enumerate() {
            println!("Line {i}: {line:?}");
        }
        println!("======================\n");

        let v = &harness.tbuf().lines()[18];
        // Check the last line of the content in the frame. "X" is uninitialized space in the
        // render buffer, so this means that the content didn't entirely fill the frame.
        assert!(!v.contains("X"));
        Ok(())
    }
}
