use canopy::{
    derive_commands,
    event::key,
    geom::Expanse,
    tutils::{buf, harness::Harness},
    widgets::frame,
    *,
};

/// Minimal version of framegym TestPattern
#[derive(StatefulNode)]
struct MinimalPattern {
    state: NodeState,
    size: Expanse,
}

#[derive_commands]
impl MinimalPattern {
    fn new() -> Self {
        MinimalPattern {
            state: NodeState::default(),
            size: Expanse::new(100, 100),
        }
    }

    #[command]
    fn scroll_down(&mut self, c: &mut dyn Context) {
        println!("MinimalPattern scroll_down called!");
        println!("  Before: view = {:?}", self.vp().view());
        c.scroll_down(self);
        println!("  After: view = {:?}", self.vp().view());
    }
}

impl Node for MinimalPattern {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        println!("MinimalPattern layout: sz = {sz:?}");
        let canvas_size = self.size;
        l.size(self, canvas_size, sz)?;
        println!("  After l.size: view = {:?}", self.vp().view());
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let view = vp.view();

        // First line shows scroll position
        let line1 = format!(
            "View: ({},{}) Canvas: {}x{}",
            view.tl.x,
            view.tl.y,
            vp.canvas().w,
            vp.canvas().h
        );
        r.text("red", view.line(0), &line1)?;

        // Show pattern that should shift when scrolling
        for y in 1..view.h {
            let abs_y = view.tl.y + (y - 1); // Adjust for starting at y=1
            let ch = ((abs_y % 26) as u8 + b'a') as char;
            let line = format!(
                "{:width$}",
                ch.to_string().repeat(4),
                width = view.w as usize
            );
            r.text("text", view.line(y), &line)?;
        }

        Ok(())
    }
}

#[derive(StatefulNode)]
struct MinimalFrameGym {
    state: NodeState,
    child: frame::Frame<MinimalPattern>,
}

impl Loader for MinimalFrameGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<MinimalFrameGym>();
        c.add_commands::<MinimalPattern>();
    }
}

impl Node for MinimalFrameGym {
    fn accept_focus(&mut self) -> bool {
        false
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        println!("MinimalFrameGym layout: sz = {sz:?}");
        self.child.layout(l, sz)?;
        self.wrap(self.child.vp())?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        Ok(())
    }
}

#[derive_commands]
impl MinimalFrameGym {
    fn new() -> Self {
        MinimalFrameGym {
            state: NodeState::default(),
            child: frame::Frame::new(MinimalPattern::new()),
        }
    }
}

#[test]
fn test_minimal_framegym_scrolling() -> Result<()> {
    let mut harness = Harness::with_size(MinimalFrameGym::new(), Expanse::new(20, 10))?;

    // Bind keys
    let cnpy = harness.canopy();
    cnpy.bind_key(key::KeyCode::Down, "", "minimal_pattern::scroll_down()")?;

    // Initial render
    println!("\n=== Initial render ===");
    harness.render()?;

    // Debug: print actual buffer content
    println!("\nActual buffer content after initial render:");
    for (i, line) in harness.buf().lines().iter().enumerate() {
        let line_str = line.trim();
        if !line_str.is_empty() {
            println!("  Line {i}: '{line_str}'");
        }
    }

    // Check initial state
    assert!(buf::contains_text(harness.buf(), "View: (0,0)"));
    assert!(buf::contains_text(harness.buf(), "aaaa")); // First content line

    // Send down key
    println!("\n=== Sending Down key ===");
    harness.key(key::KeyCode::Down)?;

    // Check if scrolled
    let buf = harness.buf();
    println!("\n=== After scroll ===");
    println!(
        "Buffer contains 'View: (0,1)': {}",
        buf::contains_text(buf, "View: (0,1)")
    );
    println!(
        "Buffer contains 'bbbb': {}",
        buf::contains_text(buf, "bbbb")
    );

    // Debug: print actual buffer content
    println!("\nActual buffer content:");
    for (i, line) in buf.lines().iter().enumerate() {
        let line_str = line.trim();
        if !line_str.is_empty() {
            println!("  Line {i}: '{line_str}'");
        }
    }

    assert!(
        buf::contains_text(buf, "View: (0,1)"),
        "Should have scrolled to (0,1)"
    );
    // After scrolling down by 1, the first pattern line should now show 'b'
    assert!(
        buf::contains_text(buf, "bbbb"),
        "Should show 'b' pattern after scrolling"
    );

    Ok(())
}
