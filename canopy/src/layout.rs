//! Helper functions for `Node::layout` implementations.

use crate::{
    geom::{Frame, Rect},
    Node, Result, ViewPort,
};

/// Adjust a node so that it fits a viewport. This fits the node to the
/// viewport's virtual size, then adjusts the node's view to place as much of it
/// within the viewport's screen rectangle as possible.
pub fn fit(n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
    let fit = n.fit(parent_vp.size())?;
    n.set_viewport(n.vp().update(fit, parent_vp.screen_rect()));
    Ok(())
}

/// Adjust a node so that viewport's screen rectangle frames it with a given
/// margin. Fits the child to the viewport screen rect minus the border margin,
/// then adjusts the node's view to place as much of of it on screen as
/// possible. This function returns a `Frame` object that can be used to draw a
/// border around the node.
pub fn frame(n: &mut dyn Node, parent_vp: ViewPort, border: u16) -> Result<Frame> {
    let fit = n.fit(parent_vp.screen_rect().inner(border).into())?;
    let screen = parent_vp.screen_rect().inner(border);
    n.update_viewport(&|vp| vp.update(fit, screen));
    // Return a frame for drawing the screen boundary, but in the view
    // rect's co-ordinate system.
    Ok(Frame::new(
        parent_vp.screen_rect().at(parent_vp.view_rect().tl),
        border,
    ))
}

/// Place a node in a given screen rectangle. This fits the node to the
/// region, and updates its viewport.
pub fn place(n: &mut dyn Node, screen: Rect) -> Result<()> {
    let fit = n.fit(screen.expanse())?;
    n.update_viewport(&|vp| vp.update(fit, screen));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Expanse, Rect},
        tutils::*,
        StatefulNode,
    };

    #[test]
    fn node_fit() -> Result<()> {
        // If the child is the same size as the parent, then wrap just produces
        // the same viewport
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, vp);

        // If the child is smaller than parent, then wrap places the viewport at
        // (0, 0)
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.update_viewport(&|vp| vp.view_right().view_down());
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at
        // (0, 0).
        let mut n = TFixed::new(20, 20);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );
        // The child can shift its view freely
        n.update_viewport(&|x| x.view_right().view_down());
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // And subsequent wraps maintain the child view position, if possible
        fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 10, 10), (10, 10))?
        );
        // When the parent viewport shrinks, we maintain position and resize
        let shrink = ViewPort::new(Expanse::new(3, 3), Rect::new(0, 0, 2, 2), (10, 10))?;
        fit(&mut n, shrink)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(1, 1, 2, 2), (10, 10))?
        );

        Ok(())
    }

    #[test]
    fn node_frame() -> Result<()> {
        // If we have room, the adjustment just shifts the child node relative to the screen.
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (11, 11))?
        );

        // If if the child node is too large, it is clipped to the bottom and left
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 8, 8), (11, 11))?
        );

        // If if the parent is smaller than the frame would require, we get a zero view
        let mut n = TFixed::new(10, 10);
        let vp = ViewPort::new(Expanse::new(0, 0), Rect::new(0, 0, 0, 0), (10, 10))?;
        frame(&mut n, vp, 1)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 0, 0), (0, 0))?
        );

        Ok(())
    }
}
