use canopy::{derive_commands, geom::Expanse, tutils::Harness, widgets::frame, *};

/// A test widget that creates a large scrollable area
#[derive(StatefulNode)]
struct LargeContent {
    state: NodeState,
    canvas_size: Expanse,
}

#[derive_commands]
impl LargeContent {
    fn new(width: u16, height: u16) -> Self {
        LargeContent {
            state: NodeState::default(),
            canvas_size: Expanse::new(width, height),
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
}

impl Node for LargeContent {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        let canvas = self.canvas_size;
        l.size(self, canvas, sz)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let view = vp.view();

        // Render a pattern that shows current position
        for y in 0..view.h {
            let absolute_y = view.tl.y + y;
            if absolute_y >= self.canvas_size.h {
                break;
            }

            let mut line = String::new();
            for x in 0..view.w {
                let absolute_x = view.tl.x + x;
                if absolute_x >= self.canvas_size.w {
                    line.push(' ');
                    continue;
                }

                // Create a pattern that shows position
                if absolute_x == 0 {
                    line.push(char::from_u32((absolute_y % 10) as u32 + '0' as u32).unwrap_or('?'));
                } else if absolute_y == 0 {
                    line.push(char::from_u32((absolute_x % 10) as u32 + '0' as u32).unwrap_or('?'));
                } else {
                    line.push('.');
                }
            }

            r.text("text", view.line(y), &line)?;
        }
        Ok(())
    }
}

impl Loader for LargeContent {
    fn load(c: &mut Canopy) {
        c.add_commands::<LargeContent>();
    }
}

#[test]
fn test_frame_basic_layout() -> Result<()> {
    // Test that we can construct a frame with content
    let content = LargeContent::new(100, 100);
    let frame = frame::Frame::new(content);

    // Test basic configuration
    assert!(frame.title.is_none());

    Ok(())
}

#[test]
fn test_frame_with_harness() -> Result<()> {
    #[derive(StatefulNode)]
    struct FramedContent {
        state: NodeState,
        frame: frame::Frame<LargeContent>,
    }

    impl Loader for FramedContent {
        fn load(c: &mut Canopy) {
            c.add_commands::<FramedContent>();
            c.add_commands::<LargeContent>();
        }
    }

    impl Node for FramedContent {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            self.frame.layout(l, sz)?;
            self.wrap(self.frame.vp())?;
            Ok(())
        }

        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.frame)
        }
    }

    #[derive_commands]
    impl FramedContent {
        fn new() -> Self {
            FramedContent {
                state: NodeState::default(),
                frame: frame::Frame::new(LargeContent::new(100, 100)),
            }
        }
    }

    let mut harness = Harness::with_size(FramedContent::new(), Expanse::new(20, 10))?;
    harness.render()?;

    // Verify frame renders without panic
    let buf = harness.buf();

    // Should contain frame characters
    assert!(buf.contains_text("┌"));
    assert!(buf.contains_text("┐"));
    assert!(buf.contains_text("└"));
    assert!(buf.contains_text("┘"));

    Ok(())
}
