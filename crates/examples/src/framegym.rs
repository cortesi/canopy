use std::time::Duration;

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, command, derive_commands,
    error::Result,
    event::key,
    geom::{Expanse, Line},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, Size, Sizing},
    render::Render,
    widget::Widget,
    widgets::{Root, frame},
};

/// Base characters used to generate the test pattern.
const PATTERN: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

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
    /// Page down in the view.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down();
    }

    #[command]
    /// Page up in the view.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up();
    }

    /// Return the character for the test pattern at a position.
    fn generate_pattern_char(x: u32, y: u32) -> char {
        let index = ((x + y) % PATTERN.len() as u32) as usize;
        PATTERN[index] as char
    }
}

impl Widget for TestPattern {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(self.size.w, self.size.h))
    }

    fn canvas(&self, _view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        Size::new(self.size.w, self.size.h)
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let origin = view.content_origin();
        let view_width = view.content.w;
        let view_height = view.content.h;

        // The view automatically handles the visible window for us
        // We just need to render the content that's visible
        for y in 0..view_height {
            let absolute_y = view.tl.y + y;
            if absolute_y >= self.size.h {
                break;
            }

            let mut line = String::with_capacity(view_width as usize);
            for x in 0..view_width {
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

            r.text(color, Line::new(origin.x, origin.y + y, view_width), &line)?;
        }

        Ok(())
    }
}

/// Root node for the frame gym demo.
pub struct FrameGym;

impl Default for FrameGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl FrameGym {
    /// Construct a new frame gym.
    pub fn new() -> Self {
        Self
    }

    /// Ensure the frame and pattern nodes are built.
    fn ensure_tree(&self, c: &mut dyn Context) {
        if !c.children().is_empty() {
            return;
        }

        let frame_id = c
            .add_child(frame::Frame::new().with_title("Frame Gym"))
            .expect("Failed to mount frame");
        let pattern_id = c
            .add_child_to(frame_id, TestPattern::new())
            .expect("Failed to mount pattern");

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })
        .expect("Failed to configure layout");
        c.with_layout_of(frame_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })
        .expect("Failed to configure frame layout");
        c.with_layout_of(pattern_id, &mut |layout| {
            *layout = Layout::fill();
        })
        .expect("Failed to configure pattern layout");
    }
}

impl Widget for FrameGym {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
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
        // Focus navigation
        .key_command(key::KeyCode::Tab, Root::cmd_focus_next())
        // Arrow keys for scrolling
        .key_command('g', TestPattern::cmd_scroll_to_top())
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
    use canopy::{NodeId, layout::Edges, testing::harness::Harness};

    use super::*;

    fn find_node_id(harness: &Harness, name: &str) -> NodeId {
        harness
            .canopy
            .core
            .nodes
            .iter()
            .find_map(|(id, node)| (node.name == name).then_some(id))
            .unwrap_or_else(|| panic!("Missing node named '{name}'"))
    }

    #[test]
    fn test_framegym_basic() -> Result<()> {
        let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
        harness.render()?;

        let frame_id = find_node_id(&harness, "frame");
        let pattern_id = find_node_id(&harness, "test_pattern");
        let frame_view = harness.canopy.core.nodes[frame_id].view;
        let pattern_view = harness.canopy.core.nodes[pattern_id].view;
        let frame_layout = &harness.canopy.core.nodes[frame_id].layout;

        assert_eq!(pattern_view.outer.tl.x, frame_view.content.tl.x);
        assert_eq!(pattern_view.outer.tl.y, frame_view.content.tl.y);
        assert_eq!(frame_view.canvas.w, frame_view.content.w);
        assert_eq!(frame_view.canvas.h, frame_view.content.h);
        assert_eq!(frame_layout.padding, Edges::all(1));
        assert_eq!(pattern_view.outer.w + 2, frame_view.outer.w);
        assert_eq!(pattern_view.outer.h + 2, frame_view.outer.h);

        let lines = harness.tbuf().lines();
        let last_col = lines[0].chars().count() - 1;
        assert_eq!(lines[0].chars().next(), Some('╭'));
        assert_eq!(lines[0].chars().nth(last_col), Some('╮'));
        assert_eq!(lines[19].chars().next(), Some('╰'));
        assert_eq!(lines[19].chars().nth(last_col), Some('╯'));

        for line in &lines[1..19] {
            assert_eq!(line.chars().next(), Some('│'));
            let right = line.chars().nth(last_col);
            assert!(matches!(right, Some('│' | '█')));
        }
        Ok(())
    }
}
