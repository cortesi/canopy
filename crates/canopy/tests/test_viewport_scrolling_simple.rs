use canopy::{
    derive_commands,
    event::key,
    geom::Expanse,
    tutils::{buf, harness::Harness},
    *,
};

/// Simple test to demonstrate viewport scrolling behavior
#[derive(StatefulNode)]
struct ScrollTest {
    state: NodeState,
}

#[derive_commands]
impl ScrollTest {
    fn new() -> Self {
        ScrollTest {
            state: NodeState::default(),
        }
    }

    #[command]
    fn scroll_down(&mut self, c: &mut dyn Context) {
        println!("Before scroll_down: view = {:?}", self.vp().view());
        c.scroll_down(self);
        println!("After scroll_down: view = {:?}", self.vp().view());
    }
}

impl Node for ScrollTest {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        println!("In layout: view before = {:?}", self.vp().view());
        // Set a large canvas (100x100) but view only shows part of it (sz)
        l.size(self, Expanse::new(100, 100), sz)?;
        println!("In layout: view after = {:?}", self.vp().view());
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        let view = vp.view();

        // Show the current scroll position
        let line1 = format!("Scroll position: ({}, {})", view.tl.x, view.tl.y);
        r.text("text", view.line(0), &line1)?;

        // Show some content that changes based on scroll position
        for y in 1..view.h.min(5) {
            let content = format!("Line {}", view.tl.y + y);
            r.text("text", view.line(y), &content)?;
        }

        Ok(())
    }
}

impl Loader for ScrollTest {
    fn load(c: &mut Canopy) {
        c.add_commands::<ScrollTest>();
    }
}

#[test]
fn test_scroll_behavior() -> Result<()> {
    let mut harness = Harness::with_size(ScrollTest::new(), Expanse::new(30, 10))?;

    // Bind scroll key
    let cnpy = harness.canopy();
    cnpy.bind_key(key::KeyCode::Down, "", "scroll_test::scroll_down()")?;

    // Initial render
    harness.render()?;
    assert!(buf::contains_text(harness.buf(), "Scroll position: (0, 0)"));
    assert!(buf::contains_text(harness.buf(), "Line 1"));

    // Send down key to trigger scroll
    harness.key(key::KeyCode::Down)?;

    // Check if scroll worked
    assert!(buf::contains_text(harness.buf(), "Scroll position: (0, 1)"));
    assert!(buf::contains_text(harness.buf(), "Line 2")); // Should now show Line 2 at the top

    Ok(())
}
