use canopy::{
    Binder, Canopy, Context, Layout, Loader, command, derive_commands,
    error::Result,
    event::key,
    geom::Expanse,
    node::Node,
    render::Render,
    state::{NodeState, StatefulNode},
    widgets::{Root, frame},
};

/// A widget that renders a test pattern.
#[derive(canopy::StatefulNode)]
pub struct TestPattern {
    /// Node state.
    state: NodeState,
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
            state: NodeState::default(),
            size: Expanse::new(500, 500),
        }
    }

    #[command]
    /// Scroll to the top-left corner.
    pub fn scroll_to_top(&mut self, c: &mut dyn Context) {
        c.scroll_to(self, 0, 0);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    #[command]
    /// Page down in the viewport.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    #[command]
    /// Page up in the viewport.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
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

impl Node for TestPattern {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
        let canvas_size = self.size;
        self.fit_size(canvas_size, sz);
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let view = vp.view();

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
}

#[derive(canopy::StatefulNode)]
/// Root node for the frame gym demo.
pub struct FrameGym {
    /// Node state.
    state: NodeState,
    /// Framed test pattern.
    child: frame::Frame<TestPattern>,
}

#[derive_commands]
impl Default for FrameGym {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameGym {
    /// Construct a new frame gym.
    pub fn new() -> Self {
        Self {
            state: NodeState::default(),
            child: frame::Frame::new(TestPattern::new()).with_title("Frame Gym".to_string()),
        }
    }
}

impl Node for FrameGym {
    fn accept_focus(&mut self) -> bool {
        false // Don't accept focus, let it go to the child
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.child.layout(l, sz)?;
        self.wrap(self.child.vp())?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
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
        .defaults::<Root<FrameGym>>()
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
        .key(key::KeyCode::PageUp, "test_pattern::page_up()")
        .key(' ', "test_pattern::page_down()")
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
