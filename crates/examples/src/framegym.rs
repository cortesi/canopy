use canopy::{
    Binder, Canopy, Context, Loader, ReadContext, Widget, command,
    commands::{ScrollDirection, VerticalDirection},
    derive_commands,
    error::Result,
    event::key,
    geom::{Expanse, Line},
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement, Size, Sizing},
    render::Render,
};
use canopy_widgets::{Frame, Root};

/// Base characters used to generate the test pattern.
const PATTERN: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
/// Key for the frame node.
pub(crate) const KEY_FRAME: &str = "frame";
/// Key for the test pattern node.
pub(crate) const KEY_PATTERN: &str = "test_pattern";

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
    /// Scroll to an absolute content position.
    pub fn scroll_to(&mut self, c: &mut dyn Context, x: u32, y: u32) {
        c.scroll_to(x, y);
    }

    /// Scroll by one line in the specified direction.
    pub fn scroll(&mut self, c: &mut dyn Context, dir: ScrollDirection) {
        match dir {
            ScrollDirection::Up => c.scroll_up(),
            ScrollDirection::Down => c.scroll_down(),
            ScrollDirection::Left => c.scroll_left(),
            ScrollDirection::Right => c.scroll_right(),
        };
    }

    /// Page in the specified direction.
    pub fn page(&mut self, c: &mut dyn Context, dir: VerticalDirection) {
        match dir {
            VerticalDirection::Up => c.page_up(),
            VerticalDirection::Down => c.page_down(),
        };
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Up);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Down);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Left);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Right);
    }

    #[command]
    /// Page up by one screen.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Up);
    }

    #[command]
    /// Page down by one screen.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Down);
    }

    /// Return the character for the test pattern at a position.
    fn generate_pattern_char(x: u32, y: u32) -> char {
        let index = ((x + y) % PATTERN.len() as u32) as usize;
        PATTERN[index] as char
    }
}

impl Widget for TestPattern {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
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

    fn render(&mut self, r: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
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
}

impl Widget for FrameGym {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_child_keyed(KEY_FRAME, Frame::new().with_title("Frame Gym"))?;
        let pattern_id = c.add_child_to_keyed(frame_id, KEY_PATTERN, TestPattern::new())?;

        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        c.with_layout_of(frame_id, &mut |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })?;
        c.with_layout_of(pattern_id, &mut |layout| {
            *layout = Layout::fill();
        })?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
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
        .defaults::<Root>()
        // Focus navigation
        .with_path("frame_gym")
        .key_command(key::KeyCode::Tab, Root::cmd_focus_next())
        // Arrow keys for scrolling
        .key_command('g', TestPattern::cmd_scroll_to().call_with([0u32, 0u32]))
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
