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
        parent.vp_mut().canvas = vp.canvas;
        parent.vp_mut().view = vp.view;
        Ok(())
    }

    /// Frame a single child node. First, we calculate the inner size after subtracting the frame. We then fit the child
    /// into this inner size, and project it appropriately in the parent view.
    pub fn frame(&self, child: &mut dyn Node, sz: Expanse, border: u16) -> Result<Frame> {
        child.layout(
            self,
            Expanse {
                w: sz.w - (border * 2),
                h: sz.h - (border * 2),
            },
        )?;
        child.vp_mut().position = crate::geom::Point {
            x: border,
            y: border,
        };
        Ok(crate::geom::Frame::new(sz.rect(), border))
    }

    /// Place a node in a given sub-rectangle of a parent's view.
    pub fn fill(&self, n: &mut dyn Node, sz: Expanse) -> Result<()> {
        let vp = n.vp_mut();
        vp.canvas = sz;
        vp.view = sz.rect();
        Ok(())
    }

    /// Place a child in a given sub-rectangle of a parent's view.
    pub fn place(&self, child: &mut dyn Node, loc: Rect) -> Result<()> {
        child.layout(self, loc.expanse())?;
        child.vp_mut().position = loc.tl;
        Ok(())
    }

    pub fn size(&self, n: &mut dyn Node, sz: Expanse, view_size: Expanse) -> Result<()> {
        n.vp_mut().fit_size(sz, view_size);
        Ok(())
    }

    /// Adjust a child node so that it fits a viewport. This lays the node out to the viewport's screen rectangle, then
    /// adjusts the node's view to place as much of it within the viewport's screen rectangle as possible.
    pub fn fit(&self, n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
        n.layout(self, parent_vp.screen_rect().into())?;
        n.vp_mut().position = parent_vp.position;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use crate::{
    //     geom::{Expanse, Rect},
    //     tutils::*,
    //     StatefulNode,
    // };

    // #[test]
    // fn node_fit() -> Result<()> {
    //     // If the child is the same size as the parent, then wrap just produces
    //     // the same viewport
    //     let mut n = TFixed::new(10, 10);
    //     let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
    //     fit(&mut n, vp)?;
    //     assert_eq!(n.state().viewport, vp);

    //     // If the child is smaller than parent, then wrap places the viewport at (0, 0)
    //     let mut n = TFixed::new(5, 5);
    //     let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
    //     let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
    //     fit(&mut n, vp)?;
    //     assert_eq!(n.state().viewport, expected,);
    //     n.vp_mut().scroll_right();
    //     n.vp_mut().scroll_down();
    //     assert_eq!(n.state().viewport, expected,);

    //     // If the child is larger than parent, then wrap places the viewport at (0, 0).
    //     let mut n = TFixed::new(20, 20);
    //     let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
    //     fit(&mut n, vp)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
    //     );

    //     // The child can shift its view freely
    //     n.vp_mut().scroll_right();
    //     n.vp_mut().scroll_down();
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
    //     );

    //     // And subsequent wraps maintain the child view position, if possible
    //     fit(&mut n, vp)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
    //     );

    //     // When the parent viewport shrinks, we maintain position and resize
    //     let shrink = ViewPort::new(Expanse::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
    //     fit(&mut n, shrink)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 2, 2), (10, 10))?
    //     );

    //     Ok(())
    // }

    // #[test]
    // fn node_frame() -> Result<()> {
    //     // If we have room, the adjustment just shifts the child node relative to the screen.
    //     let mut n = TFixed::new(5, 5);
    //     let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
    //     frame(&mut n, vp, 1)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (11, 11))?
    //     );

    //     // If if the child node is too large, it is clipped to the bottom and left
    //     let mut n = TFixed::new(10, 10);
    //     let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
    //     frame(&mut n, vp, 1)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 8, 8), (11, 11))?
    //     );

    //     // If if the parent is smaller than the frame would require, we get a zero view
    //     let mut n = TFixed::new(10, 10);
    //     let vp = ViewPort::new(Expanse::new(0, 0), Rect::new(0, 0, 0, 0), (10, 10))?;
    //     frame(&mut n, vp, 1)?;
    //     assert_eq!(
    //         n.state().viewport,
    //         ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 0, 0), (0, 0))?
    //     );

    //     Ok(())
    // }
}
