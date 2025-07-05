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
        Context, Expanse, Node, NodeState, Result, StatefulNode,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    };

    /// A simple scrollable test widget for testing frame scrolling
    #[derive(StatefulNode)]
    struct ScrollableContent {
        state: NodeState,
        canvas_size: Expanse,
    }

    impl ScrollableContent {
        fn new(width: u16, height: u16) -> Self {
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
                    let ch = char::from_u32(((absolute_x + absolute_y) % 10) as u32 + '0' as u32)
                        .unwrap_or('?');
                    line.push(ch);
                }

                r.text("text", view.line(y), &line)?;
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
}
