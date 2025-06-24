//! Helper functions for `Node::layout` implementations.

use crate::{
    geom::{Expanse, Frame, Rect},
    Node, Result, ViewPort,
};

pub struct Layout {}

impl Layout {
    /// Wrap a single child node, mirroring the child's size and view.
    pub fn wrap(&self, parent: &mut dyn Node, vp: ViewPort) -> Result<()> {
        // Mirror the child's size and view
        parent.state_mut().set_canvas(vp.canvas);
        parent.state_mut().set_view(vp.view);
        Ok(())
    }

    /// Frame a single child node. First, we calculate the inner size after subtracting the frame. We then fit the child
    /// into this inner size, and project it appropriately in the parent view.
    pub fn frame(&self, child: &mut dyn Node, sz: Expanse, border: u16) -> Result<Frame> {
        child.state_mut().set_position(crate::geom::Point {
            x: border,
            y: border,
        });
        child.layout(
            self,
            Expanse {
                w: sz.w.saturating_sub(border * 2),
                h: sz.h.saturating_sub(border * 2),
            },
        )?;
        Ok(crate::geom::Frame::new(sz.rect(), border))
    }

    /// Place a node in a given sub-rectangle of a parent's view.
    pub fn fill(&self, n: &mut dyn Node, sz: Expanse) -> Result<()> {
        n.state_mut().set_canvas(sz);
        n.state_mut().set_view(sz.rect());
        Ok(())
    }

    /// Place a child in a given sub-rectangle of a parent's view.
    pub fn place(&self, child: &mut dyn Node, parent_vp: ViewPort, loc: Rect) -> Result<()> {
        child
            .state_mut()
            .set_position(parent_vp.position.scroll(loc.tl.x as i16, loc.tl.y as i16));
        child.layout(self, loc.expanse())?;
        child.state_mut().constrain(parent_vp);
        Ok(())
    }

    pub fn size(&self, n: &mut dyn Node, sz: Expanse, view_size: Expanse) -> Result<()> {
        n.state_mut().fit_size(sz, view_size);
        Ok(())
    }

    /// Adjust a child node so that it fits a viewport. This lays the node out to the viewport's screen rectangle, then
    /// adjusts the node's view to place as much of it within the viewport's screen rectangle as possible.
    pub fn fit(&self, n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
        n.layout(self, parent_vp.screen_rect().into())?;
        n.state_mut().set_position(parent_vp.position);
        n.state_mut().constrain(parent_vp);
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn node_fit() -> Result<()> {
        // If the child is the same size as the parent, then wrap just produces
        // the same viewport
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let l = Layout {};
        l.fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, vp);

        // If the child is smaller than parent, then wrap places the viewport at (0, 0)
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        l.fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.state_mut().scroll_right();
        n.state_mut().scroll_down();
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at (0, 0).
        let mut n = TFixed::new(20, 20);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        l.fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );

        // The child can shift its view freely
        n.state_mut().scroll_right();
        n.state_mut().scroll_down();
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );

        // Subsequent fits reset the child view position
        l.fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );

        // When the parent viewport shrinks, the view is clamped
        let shrink = ViewPort::new(Expanse::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
        l.fit(&mut n, shrink)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 2, 2), (10, 10))?
        );

        Ok(())
    }

    use super::*;
    use crate::{
        self as canopy,
        geom::{Expanse, Frame, Point, Rect},
        tutils::TFixed,
        Canopy, Context, Node, NodeState, Render, StatefulNode, *,
    };

    #[test]
    fn frame_does_not_overflow_small_parent() -> Result<()> {
        let l = Layout {};
        let mut child = TFixed::new(2, 2);
        let f = l.frame(&mut child, Expanse::new(1, 1), 1)?;
        assert_eq!(f, Frame::zero());
        assert_eq!(child.vp().position, Point { x: 1, y: 1 });
        Ok(())
    }

    #[derive(StatefulNode)]
    struct Big {
        state: NodeState,
    }

    impl Big {
        fn new() -> Self {
            Big {
                state: NodeState::default(),
            }
        }
    }

    #[derive_commands]
    impl Big {}

    impl Node for Big {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, Expanse::new(sz.w * 2, sz.h * 2))
        }

        fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
            r.fill("", self.vp().canvas.rect(), 'x')
        }
    }

    #[derive(StatefulNode)]
    struct Root {
        state: NodeState,
        child: Big,
    }

    impl Root {
        fn new() -> Self {
            Root {
                state: NodeState::default(),
                child: Big::new(),
            }
        }
    }

    #[derive_commands]
    impl Root {}

    impl Node for Root {
        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.child)
        }

        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
            let vp = self.vp();
            let loc = Rect::new(sz.w.saturating_sub(1), sz.h.saturating_sub(1), sz.w, sz.h);
            l.place(&mut self.child, vp, loc)?;
            Ok(())
        }
    }

    #[test]
    fn child_clamped_to_parent() -> Result<()> {
        use crate::backend::test::CanvasRender;

        let size = Expanse::new(4, 4);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();

        canopy.set_root_size(size, &mut root)?;
        canopy.render(&mut cr, &mut root)?;

        let parent = root.vp().screen_rect();
        let child = root.child.vp().screen_rect();
        assert!(parent.contains_rect(&child));

        let canvas = buf.lock().unwrap();
        for y in 0..size.h {
            for x in 0..size.w {
                let ch = canvas.cells[y as usize][x as usize];
                if child.contains_point((x, y)) {
                    assert_eq!(ch, 'x');
                } else {
                    assert_eq!(ch, ' ');
                }
            }
        }
        Ok(())
    }

    #[test]
    fn node_frame() -> Result<()> {
        // If we have room, the adjustment just shifts the child node relative to the screen.
        let mut n = TFixed::new(5, 5);
        let l = Layout {};
        l.frame(&mut n, Expanse::new(10, 10), 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (1, 1))?
        );

        // If if the child node is too large, it is clipped to the bottom and left
        let mut n = TFixed::new(10, 10);
        l.frame(&mut n, Expanse::new(10, 10), 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (1, 1))?
        );

        // If if the parent is smaller than the frame would require, we get a zero view
        let mut n = TFixed::new(10, 10);
        l.frame(&mut n, Expanse::new(0, 0), 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (1, 1))?
        );

        Ok(())
    }
}
