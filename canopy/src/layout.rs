//! Helper functions for `Node::layout` implementations.

use crate::{
    geom::{Frame, Rect},
    Node, Result, ViewPort,
};

/// Wrap a single child node - fit the child into the size and project it to the point (0, 0) in the parent view. We
/// then mirror the child's size and view into the parent.
#[macro_export]
macro_rules! fit_wrap {
    ($self: ident, $child:expr, $sz:expr) => {
        $child.fit($sz)?;
        $child.vp_mut().position = (0, 0).into();

        let cvp = $child.vp();
        // Mirror the child's size and view
        $self.vp_mut().canvas = cvp.canvas;
        $self.vp_mut().view = cvp.view;
    };
}

/// Frame a single child node. First, we calculate the inner size after subtracting the frame. We then fit the child
/// into this inner size, and project it appropriately in the parent view.
#[macro_export]
macro_rules! fit_frame {
    ($self: ident, $child:expr, $sz:expr, $border:expr) => {{
        $child.fit(Expanse {
            w: $sz.w - ($border * 2),
            h: $sz.h - ($border * 2),
        })?;
        $child.vp_mut().position = crate::geom::Point {
            x: $border,
            y: $border,
        };
        $self.vp_mut().canvas = $sz;
        $self.vp_mut().view = $sz.rect();
        crate::geom::Frame::new($sz.rect(), $border)
    }};
}

/// Place a child in a given sub-rectangle of our view.
#[macro_export]
macro_rules! fit_place {
    ($self: ident, $child:expr, $loc:expr) => {
        $child.fit($loc.expanse())?;
        $child.vp_mut().position = $loc.tl;
    };
}

/// Adjust a node so that it fits a viewport. This fits the node to the viewport's screen rectangle, then adjusts the
/// node's view to place as much of it within the viewport's screen rectangle as possible.
pub fn fit(n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
    n.fit(parent_vp.screen_rect().into())?;
    n.vp_mut().position = parent_vp.position;
    Ok(())
}

/// Adjust a node so that viewport's screen rectangle frames it with a given
/// margin. Fits the child to the viewport screen rect minus the border margin,
/// then adjusts the node's view to place as much of of it on screen as
/// possible. This function returns a `Frame` object that can be used to draw a
/// border around the node.
pub fn frame(n: &mut dyn Node, parent_vp: ViewPort, border: u16) -> Result<Frame> {
    let inner = parent_vp.screen_rect().inner(border);
    n.fit(inner.into())?;
    n.vp_mut().position = inner.tl;

    // Return a frame for drawing the screen boundary, but in the view
    // rect's co-ordinate system.
    Ok(Frame::new(
        parent_vp.screen_rect().at(parent_vp.view.tl),
        border,
    ))
}

/// Place a node in a given screen rectangle. This fits the node to the
/// region, and updates its viewport.
pub fn place(n: &mut dyn Node, screen: Rect) -> Result<()> {
    n.fit(screen.expanse())?;
    n.vp_mut().position = screen.tl;
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

        // If the child is smaller than parent, then wrap places the viewport at (0, 0)
        let mut n = TFixed::new(5, 5);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        let expected = ViewPort::new(Expanse::new(5, 5), Rect::new(0, 0, 5, 5), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(n.state().viewport, expected,);
        n.vp_mut().scroll_right();
        n.vp_mut().scroll_down();
        assert_eq!(n.state().viewport, expected,);

        // If the child is larger than parent, then wrap places the viewport at (0, 0).
        let mut n = TFixed::new(20, 20);
        let vp = ViewPort::new(Expanse::new(10, 10), Rect::new(0, 0, 10, 10), (10, 10))?;
        fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 10, 10), (10, 10))?
        );

        // The child can shift its view freely
        n.vp_mut().scroll_right();
        n.vp_mut().scroll_down();
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
