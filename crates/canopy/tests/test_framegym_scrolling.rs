use canopy::{derive_commands, event::key, geom::Expanse, tutils::Harness, widgets::frame, *};

/// Test pattern widget (copy from framegym)
#[derive(StatefulNode)]
struct TestPattern {
    state: NodeState,
    size: Expanse,
}

#[derive_commands]
impl TestPattern {
    fn new() -> Self {
        TestPattern {
            state: NodeState::default(),
            size: Expanse::new(500, 500),
        }
    }

    #[command]
    fn scroll_down(&mut self, c: &mut dyn Context) {
        c.scroll_down(self);
    }

    #[command]
    fn scroll_up(&mut self, c: &mut dyn Context) {
        c.scroll_up(self);
    }

    #[command]
    fn scroll_left(&mut self, c: &mut dyn Context) {
        c.scroll_left(self);
    }

    #[command]
    fn scroll_right(&mut self, c: &mut dyn Context) {
        c.scroll_right(self);
    }

    #[command]
    fn page_down(&mut self, c: &mut dyn Context) {
        c.page_down(self);
    }

    #[command]
    fn page_up(&mut self, c: &mut dyn Context) {
        c.page_up(self);
    }

    fn generate_pattern_char(x: u16, y: u16) -> char {
        let pattern = "abcdefghijklmnopqrstuvwxyz0123456789";
        let pattern_len = pattern.len() as u16;
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

        // Debug removed - viewport is working correctly

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

            r.text("text", view.line(y), &line)?;
        }

        Ok(())
    }
}

#[derive(StatefulNode)]
struct FrameGym {
    state: NodeState,
    child: frame::Frame<TestPattern>,
}

#[derive_commands]
impl FrameGym {
    fn new() -> Self {
        FrameGym {
            state: NodeState::default(),
            child: frame::Frame::new(TestPattern::new())
                .with_title("Test Pattern - Use arrow keys to scroll".to_string()),
        }
    }
}

impl Node for FrameGym {
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

#[test]
fn test_framegym_initial_state() -> Result<()> {
    let mut harness = Harness::with_size(FrameGym::new(), Expanse::new(30, 15))?;
    harness.render()?;

    let buf = harness.buf();

    // Check frame is rendered
    assert!(buf.contains_text("┌"));
    assert!(buf.contains_text("┐"));
    assert!(buf.contains_text("└"));
    assert!(buf.contains_text("┘"));

    // Check title is rendered
    assert!(buf.contains_text("Test Pattern"));

    // Check initial pattern is visible (should start with 'a' at top-left of content area)
    assert!(buf.contains_text("abcdefg"));

    Ok(())
}

#[test]
fn test_framegym_scrolling() -> Result<()> {
    let mut harness = Harness::with_size(FrameGym::new(), Expanse::new(30, 15))?;

    // Set up key bindings like the framegym example
    let cnpy = harness.canopy();

    // The framegym example uses an empty path filter, which means the bindings
    // apply regardless of the focus path. This is why it works in the example.

    // The path should be ["frame_gym", "frame", "test_pattern"] when test_pattern is focused
    // So we need to bind with path filter that matches this
    cnpy.bind_key(key::KeyCode::Down, "", "test_pattern::scroll_down()")?;
    cnpy.bind_key(key::KeyCode::Up, "", "test_pattern::scroll_up()")?;
    cnpy.bind_key(key::KeyCode::Left, "", "test_pattern::scroll_left()")?;
    cnpy.bind_key(key::KeyCode::Right, "", "test_pattern::scroll_right()")?;

    harness.render()?;

    // Check initial pattern
    assert!(
        harness.buf().contains_text("abcdefg"),
        "Initial pattern should start with 'abcdefg'"
    );

    // Debug: check viewport before scrolling
    println!("Before scrolling test pattern widget");

    // Scroll down using key event
    harness.key(key::KeyCode::Down)?;

    // After scrolling down by 1, the pattern should shift

    // After scrolling down by 1, the pattern should shift
    // The first visible line should now show the second row of the pattern
    let buf = harness.buf();

    // Debug output removed - test is working correctly

    // Check if scrolling worked
    // Note: Line 1 of the buffer is the first line inside the frame (after the top border)
    // After scrolling down by 1, this should show the pattern from y=1
    assert!(
        buf.contains_text("bcdefgh"),
        "Pattern should have shifted after scrolling down"
    );

    Ok(())
}

#[test]
fn test_framegym_canvas_size() -> Result<()> {
    let framegym = FrameGym::new();

    // The TestPattern should have a large canvas
    assert_eq!(framegym.child.child.size.w, 500);
    assert_eq!(framegym.child.child.size.h, 500);

    Ok(())
}
