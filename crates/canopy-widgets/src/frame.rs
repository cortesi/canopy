use canopy_core as canopy;

use pad::PadStr;

use canopy_core::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, derive_commands, geom,
};

/// Defines the set of glyphs used to draw the frame
pub struct FrameGlyphs {
    pub topleft: char,
    pub topright: char,
    pub bottomleft: char,
    pub bottomright: char,
    pub horizontal: char,
    pub vertical: char,
    pub vertical_active: char,
    pub horizontal_active: char,
}

/// Single line thin Unicode box drawing frame set
pub const SINGLE: FrameGlyphs = FrameGlyphs {
    topleft: '┌',
    topright: '┐',
    bottomleft: '└',
    bottomright: '┘',
    horizontal: '─',
    vertical: '│',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Double line Unicode box drawing frame set
pub const DOUBLE: FrameGlyphs = FrameGlyphs {
    topleft: '╔',
    topright: '╗',
    bottomleft: '╚',
    bottomright: '╝',
    horizontal: '═',
    vertical: '║',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// Single line thick Unicode box drawing frame set
pub const SINGLE_THICK: FrameGlyphs = FrameGlyphs {
    topleft: '┏',
    topright: '┓',
    bottomleft: '┗',
    bottomright: '┛',
    horizontal: '━',
    vertical: '┃',
    horizontal_active: '▄',
    vertical_active: '█',
};

/// A frame around an element.
///
/// Colors:
///     frame:          normal frame border
///     frame/focused   frame border if we hold focus
///     frame/active    color of active area indicator
#[derive(canopy_core::StatefulNode)]
pub struct Frame<N>
where
    N: Node,
{
    pub child: N,
    pub state: NodeState,
    pub glyphs: FrameGlyphs,
    pub title: Option<String>,
    pub frame: geom::Frame,
}

#[derive_commands]
impl<N> Frame<N>
where
    N: Node,
{
    pub fn new(c: N) -> Self {
        Frame {
            child: c,
            state: NodeState::default(),
            glyphs: SINGLE,
            title: None,
            frame: geom::Frame::zero(),
        }
    }

    /// Build a frame with a specified glyph set
    pub fn with_glyphs(mut self, glyphs: FrameGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a frame with a specified title
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }
}

impl<N> Node for Frame<N>
where
    N: Node,
{
    fn force_render(&self, c: &dyn Context) -> bool {
        c.needs_render(&self.child)
    }

    fn layout(&mut self, l: &Layout, sz: canopy_core::geom::Expanse) -> Result<()> {
        // We are always exactly the layout size
        self.state_mut().set_canvas(sz);
        self.state_mut().set_view(sz.rect());

        self.frame = canopy_core::geom::Frame::new(sz.rect(), 1);
        let inner = self.frame.inner();
        l.place_(&mut self.child, inner)?;
        Ok(())
    }

    fn render(&mut self, c: &dyn Context, rndr: &mut Render) -> Result<()> {
        let f = self.frame;
        let style = if c.is_on_focus_path(self) {
            "frame/focused"
        } else {
            "frame"
        };

        rndr.fill(style, f.topleft, self.glyphs.topleft)?;
        rndr.fill(style, f.topright, self.glyphs.topright)?;
        rndr.fill(style, f.bottomleft, self.glyphs.bottomleft)?;
        rndr.fill(style, f.bottomright, self.glyphs.bottomright)?;
        rndr.fill(style, f.left, self.glyphs.vertical)?;

        if let Some(title) = &self.title {
            title.pad(
                f.top.w as usize,
                self.glyphs.horizontal,
                pad::Alignment::Left,
                true,
            );
            rndr.text(style, f.top.line(0), title)?;
        } else {
            rndr.fill(style, f.top, self.glyphs.horizontal)?;
        }

        if let Some((pre, active, post)) = self.child.vp().vactive(f.right)? {
            rndr.fill(style, pre, self.glyphs.vertical)?;
            rndr.fill(style, post, self.glyphs.vertical)?;
            rndr.fill(style, active, self.glyphs.vertical_active)?;
        } else {
            rndr.fill(style, f.right, self.glyphs.vertical)?;
        }

        if let Some((pre, active, post)) = self.child.vp().hactive(f.bottom)? {
            rndr.fill(style, pre, self.glyphs.horizontal)?;
            rndr.fill(style, post, self.glyphs.horizontal)?;
            rndr.fill(style, active, self.glyphs.horizontal_active)?;
        } else {
            rndr.fill(style, f.bottom, self.glyphs.horizontal)?;
        }

        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canopy_core::{
        Context, Expanse, Node, NodeState, Result, StatefulNode, TermBuf, ViewStack,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        style::{StyleManager, StyleMap},
        tutils::{buf::BufTest, dummyctx::DummyContext},
    };

    /// A simple scrollable test widget for testing frame scrolling
    #[derive(StatefulNode)]
    struct ScrollableContent {
        state: NodeState,
        canvas_size: Expanse,
    }

    impl ScrollableContent {
        fn new(width: u32, height: u32) -> Self {
            ScrollableContent {
                state: NodeState::default(),
                canvas_size: Expanse::new(width, height),
            }
        }
    }

    impl CommandNode for ScrollableContent {
        fn commands() -> Vec<CommandSpec> {
            vec![]
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
        }
    }

    impl Node for ScrollableContent {
        fn accept_focus(&mut self) -> bool {
            true
        }

        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            // Report our desired size (which may be larger than the given size)
            let size = self.canvas_size;
            l.size(self, size, sz)?;
            Ok(())
        }

        fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
            let vp = self.vp();
            let view = vp.view();

            // Render a test pattern that's easy to verify
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

                    // Simple pattern: character based on position
                    let ch = char::from_u32(((absolute_x + absolute_y) % 10) + '0' as u32)
                        .unwrap_or('?');
                    line.push(ch);
                }

                let target_line =
                    canopy_core::geom::Line::new(vp.position().x, vp.position().y + y, view.w);
                r.text("text", target_line, &line)?;
            }
            Ok(())
        }
    }

    #[test]
    fn test_frame_construction() {
        let content = ScrollableContent::new(50, 50);
        let frame = Frame::new(content);

        // Check default settings
        assert_eq!(frame.title, None);
        assert_eq!(frame.glyphs.topleft, '┌');
        assert_eq!(frame.glyphs.topright, '┐');
    }

    #[test]
    fn test_frame_with_title() {
        let content = ScrollableContent::new(50, 50);
        let frame = Frame::new(content).with_title("Test Title".to_string());

        assert_eq!(frame.title, Some("Test Title".to_string()));
    }

    #[test]
    fn test_frame_with_different_glyphs() {
        let content = ScrollableContent::new(10, 10);
        let frame = Frame::new(content).with_glyphs(DOUBLE);

        // Check that double-line glyphs are set
        assert_eq!(frame.glyphs.topleft, '╔');
        assert_eq!(frame.glyphs.topright, '╗');
        assert_eq!(frame.glyphs.bottomleft, '╚');
        assert_eq!(frame.glyphs.bottomright, '╝');
    }

    #[test]
    fn test_frame_child_traversal() {
        let content = ScrollableContent::new(10, 10);
        let mut frame = Frame::new(content);

        let mut child_count = 0;
        frame
            .children(&mut |_child| {
                child_count += 1;
                Ok(())
            })
            .unwrap();

        assert_eq!(child_count, 1);
    }

    // Helper function to check frame boundaries for overdraw
    fn check_frame_boundaries(buffer: &canopy_core::TermBuf, test_name: &str) {
        // Check bottom edge (row 9) - should contain frame characters
        for x in 1..9 {
            let cell = buffer.get(geom::Point { x, y: 9 });
            if let Some(cell) = cell {
                if cell.ch.is_ascii_digit() {
                    panic!(
                        "{}: Frame bottom edge at ({}, 9) was overdrawn with content character '{}'",
                        test_name, x, cell.ch
                    );
                }
            }
        }

        // Check right edge (column 9) - should contain frame characters
        for y in 1..9 {
            let cell = buffer.get(geom::Point { x: 9, y });
            if let Some(cell) = cell {
                if cell.ch.is_ascii_digit() {
                    panic!(
                        "{}: Frame right edge at (9, {}) was overdrawn with content character '{}'",
                        test_name, y, cell.ch
                    );
                }
            }
        }

        // Check corners
        let corners = [
            (0, 0, "top-left"),
            (9, 0, "top-right"),
            (0, 9, "bottom-left"),
            (9, 9, "bottom-right"),
        ];

        for (x, y, corner_name) in corners {
            let cell = buffer.get(geom::Point { x, y });
            if let Some(cell) = cell {
                if cell.ch.is_ascii_digit() {
                    panic!(
                        "{}: Frame {} corner at ({}, {}) was overdrawn with content character '{}'",
                        test_name, corner_name, x, y, cell.ch
                    );
                }
            }
        }
    }

    #[test]
    fn test_frame_overdraw_with_viewport_stack() {
        // Create the components
        let frame_size = Expanse::new(10, 10);
        let content = ScrollableContent::new(20, 20);
        let mut frame = Frame::new(content);

        let ctx = DummyContext {};
        let layout = Layout {};

        // Layout the frame
        frame.layout(&layout, frame_size).unwrap();

        // Create style components
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::default();

        // Test multiple scroll positions to find potential overdraw
        let scroll_tests = vec![
            (0, 0, "Top-left"),
            (5, 5, "Middle"),
            (10, 10, "Near bottom-right"),
            (12, 0, "Right edge"),
            (0, 12, "Bottom edge"),
            (12, 12, "Bottom-right corner"),
        ];

        for (scroll_x, scroll_y, test_name) in scroll_tests {
            println!("\n=== Testing scroll position ({scroll_x}, {scroll_y}) - {test_name} ===");
            // DummyContext doesn't actually scroll, so we need to manually set the scroll position
            frame.child.state_mut().scroll_to(scroll_x, scroll_y);

            // Create main buffer
            let mut main_buf = TermBuf::empty(frame_size);

            // Create ViewStack with screen viewport
            let screen_vp =
                canopy_core::ViewPort::new(frame_size, frame_size.rect(), geom::Point::zero())
                    .unwrap();
            let mut view_stack = ViewStack::new(screen_vp);

            // Simulate how Canopy would render this:
            // 1. First render the frame
            let frame_vp = frame.vp();
            println!(
                "Frame viewport: canvas={:?}, view={:?}, pos={:?}",
                frame_vp.canvas(),
                frame_vp.view(),
                frame_vp.position()
            );

            view_stack.push(frame_vp);

            // Frame renders to its own buffer
            let frame_view = frame_vp.view();
            let mut frame_render = Render::new(&stylemap, &mut style_manager, frame_view);
            frame.render(&ctx, &mut frame_render).unwrap();

            // Copy frame to main buffer at its screen position
            if let Some((_canvas_rect, screen_rect)) = view_stack.projection() {
                println!("Frame projection: screen_rect={screen_rect:?}");
                let frame_buf = frame_render.get_buffer();
                main_buf.copy_to_rect(frame_buf, screen_rect);
            }

            // 2. Then render each child
            frame
                .children(&mut |child| {
                    let child_vp = child.vp();
                    println!(
                        "Child viewport: canvas={:?}, view={:?}, pos={:?}",
                        child_vp.canvas(),
                        child_vp.view(),
                        child_vp.position()
                    );

                    // Push child viewport
                    view_stack.push(child_vp);

                    // Child renders to its own buffer
                    let child_view = child_vp.view();
                    let mut child_render =
                        Render::new(&stylemap, &mut style_manager,  child_view);
                    child.render(&ctx, &mut child_render)?;

                    // Copy child to main buffer at its projected screen position
                    if let Some((canvas_rect, screen_rect)) = view_stack.projection() {
                        println!(
                            "Child projection: canvas_rect={canvas_rect:?}, screen_rect={screen_rect:?}"
                        );
                        let child_buf = child_render.get_buffer();
                        println!("Child buffer first line: {:?}", BufTest::new(child_buf).line_text(0));
                        main_buf.copy_to_rect(child_buf, screen_rect);
                    } else {
                        println!("No projection for child!");
                    }

                    view_stack.pop().unwrap();
                    Ok(())
                })
                .unwrap();

            view_stack.pop().unwrap();

            // Print the result
            println!("Buffer with viewport stack:");
            for line in BufTest::new(&main_buf).lines() {
                println!("{line}");
            }

            // Check for overdraw
            check_frame_boundaries(&main_buf, &format!("Viewport stack test - {test_name}"));
        }
    }

    #[test]
    fn test_frame_overdraw_with_multiple_scrolls() {
        // Create a frame size of 10x10 with 1-pixel border
        // Inner area will be 8x8 at position (1,1)
        let frame_size = Expanse::new(10, 10);

        // Create scrollable content that's larger than the frame
        // Make it 20x20 so we can scroll it
        let content = ScrollableContent::new(20, 20);
        let mut frame = Frame::new(content);

        // Set up test context
        let ctx = DummyContext {};

        // Layout the frame
        let layout = Layout {};
        frame.layout(&layout, frame_size).unwrap();

        // Create render environment
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::default();

        // Test various scroll positions
        let test_cases = vec![
            ("Initial position", 0, 0),
            ("Scroll right edge", 12, 0),
            ("Scroll bottom edge", 0, 12),
            ("Scroll bottom-right corner", 12, 12),
            ("Scroll to middle", 6, 6),
            ("Scroll extreme bottom", 0, 14),
            ("Scroll extreme right", 14, 0),
            ("Scroll extreme corner", 14, 14),
        ];

        for (test_name, x, y) in test_cases {
            println!("\n=== Testing: {test_name} (scroll to {x}, {y}) ===");

            // Scroll to position - DummyContext doesn't actually scroll, so we do it directly
            frame.child.state_mut().scroll_to(x, y);

            // Create fresh render for this test
            let render_rect = geom::Rect::new(0, 0, 10, 10);
            let mut render = Render::new(&stylemap, &mut style_manager, render_rect);

            // Render the frame first
            frame.render(&ctx, &mut render).unwrap();

            // Then render the child content
            frame
                .children(&mut |child| child.render(&ctx, &mut render))
                .unwrap();

            // Get the buffer and print it
            let buffer = render.get_buffer();
            println!("Buffer contents:");
            for line in BufTest::new(buffer).lines() {
                println!("{line}");
            }

            // Check for overdraw
            check_frame_boundaries(buffer, test_name);
        }

        // Also test incremental scrolling
        println!("\n=== Testing incremental scrolling ===");

        // Reset to origin
        frame.child.state_mut().scroll_to(0, 0);

        // Scroll down one line at a time
        for i in 0..15 {
            frame.child.state_mut().scroll_down();

            let render_rect = geom::Rect::new(0, 0, 10, 10);
            let mut render = Render::new(&stylemap, &mut style_manager, render_rect);

            frame.render(&ctx, &mut render).unwrap();
            frame
                .children(&mut |child| child.render(&ctx, &mut render))
                .unwrap();

            let buffer = render.get_buffer();
            check_frame_boundaries(buffer, &format!("After {} scroll_down calls", i + 1));
        }

        // Scroll right one column at a time
        frame.child.state_mut().scroll_to(0, 0);
        for i in 0..15 {
            frame.child.state_mut().scroll_right();

            let render_rect = geom::Rect::new(0, 0, 10, 10);
            let mut render = Render::new(&stylemap, &mut style_manager, render_rect);

            frame.render(&ctx, &mut render).unwrap();
            frame
                .children(&mut |child| child.render(&ctx, &mut render))
                .unwrap();

            let buffer = render.get_buffer();
            check_frame_boundaries(buffer, &format!("After {} scroll_right calls", i + 1));
        }
    }
}
