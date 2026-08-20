use canopy::{
    command, derive_commands,
    geom::{Direction, Line},
    layout::CanvasContext,
    prelude::*,
};
use canopy_widgets::Frame;

/// Base characters used to generate the test pattern.
const PATTERN: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Default bindings for the frame gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind("Tab", { path = "frame_gym", description = "Next focus" }, function()
    root.focus("Next")
end)
canopy.bind("g", { path = "frame_gym", description = "Top" }, function()
    test_pattern.scroll_to(0, 0)
end)
canopy.bind("Down", { path = "frame_gym", description = "Scroll down" }, function()
    test_pattern.scroll("Down")
end)
canopy.bind("Up", { path = "frame_gym", description = "Scroll up" }, function()
    test_pattern.scroll("Up")
end)
canopy.bind("Left", { path = "frame_gym", description = "Scroll left" }, function()
    test_pattern.scroll("Left")
end)
canopy.bind("Right", { path = "frame_gym", description = "Scroll right" }, function()
    test_pattern.scroll("Right")
end)
canopy.bind("j", { path = "frame_gym", description = "Scroll down" }, function()
    test_pattern.scroll("Down")
end)
canopy.bind("k", { path = "frame_gym", description = "Scroll up" }, function()
    test_pattern.scroll("Up")
end)
canopy.bind("h", { path = "frame_gym", description = "Scroll left" }, function()
    test_pattern.scroll("Left")
end)
canopy.bind("l", { path = "frame_gym", description = "Scroll right" }, function()
    test_pattern.scroll("Right")
end)
canopy.bind("PageDown", { path = "frame_gym", description = "Page down" }, function()
    test_pattern.page(1)
end)
canopy.bind("Space", { path = "frame_gym", description = "Page down" }, function()
    test_pattern.page(1)
end)
canopy.bind("PageUp", { path = "frame_gym", description = "Page up" }, function()
    test_pattern.page(-1)
end)
canopy.bind("q", { path = "root", description = "Quit" }, function()
    root.quit()
end)
"#;

// Typed keys for keyed children
canopy::key!(FrameSlot: Frame);
canopy::key!(PatternSlot: TestPattern);

/// A widget that renders a test pattern.
pub struct TestPattern {
    /// Virtual canvas size.
    size: Size,
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
            size: Size::new(500, 500),
        }
    }

    #[command]
    /// Scroll to an absolute content position.
    pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {
        c.scroll_to(x, y);
    }

    #[command]
    /// Scroll by one line in the specified direction.
    /// @param dir The direction to scroll.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: Direction) {
        match dir {
            Direction::Up => c.scroll_up(),
            Direction::Down => c.scroll_down(),
            Direction::Left => c.scroll_left(),
            Direction::Right => c.scroll_right(),
        };
    }

    #[command]
    /// Page the view. Negative values move up; positive values move down.
    /// @param delta Signed page delta.
    pub fn page(&mut self, c: &mut dyn Context, delta: i32) {
        if delta < 0 {
            c.page_up();
        } else if delta > 0 {
            c.page_down();
        }
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

    fn canvas(&self, _view: Size, _ctx: &CanvasContext) -> Size {
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

impl FrameGym {
    /// Construct a new frame gym.
    pub fn new() -> Self {
        Self
    }
}

impl Widget for FrameGym {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_keyed::<FrameSlot>(Frame::new().with_title("Frame Gym"))?;
        let pattern_id = c.add_keyed_to(frame_id, PatternSlot::KEY, TestPattern::new())?;

        c.set_layout(Layout::fill())?;
        c.set_layout_of(pattern_id, Layout::fill())?;
        Ok(())
    }
}

impl Loader for FrameGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<TestPattern>()?;
        Ok(())
    }
}

/// Install key bindings for the frame gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)?;
    Ok(())
}
