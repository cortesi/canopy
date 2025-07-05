use canopy::{derive_commands, event::key, geom::Expanse, widgets::frame, *};

/// A widget that renders a test pattern
#[derive(StatefulNode)]
pub struct TestPattern {
    state: NodeState,
    size: Expanse,
}

#[derive_commands]
impl Default for TestPattern {
    fn default() -> Self {
        Self::new()
    }
}

impl TestPattern {
    pub fn new() -> Self {
        TestPattern {
            state: NodeState::default(),
            size: Expanse::new(500, 500),
        }
    }

    #[command]
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    #[command]
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    #[command]
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    #[command]
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
    }

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

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        let canvas_size = self.size;
        l.size(self, canvas_size, sz)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let view = vp.view();

        // The viewport automatically handles the visible window for us
        // We just need to render the content that's visible
        for y in 0..view.h.saturating_sub(1) {
            // Leave room for debug line
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

#[derive(StatefulNode)]
pub struct FrameGym {
    state: NodeState,
    child: frame::Frame<TestPattern>,
}

#[derive_commands]
impl Default for FrameGym {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameGym {
    pub fn new() -> Self {
        FrameGym {
            state: NodeState::default(),
            child: frame::Frame::new(TestPattern::new())
                .with_title("Test Pattern - Use arrow keys to scroll (Tab to focus)".to_string()),
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
        c.add_commands::<FrameGym>();
        c.add_commands::<TestPattern>();
    }
}

pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .defaults::<Root<FrameGym>>()
        .with_path("")
        // Focus navigation
        .key(key::KeyCode::Tab, "root::focus_next()")
        // Arrow keys for scrolling
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
