//! Helper functions for `Node::layout` implementations.

use crate::{
    Node, Result, ViewPort,
    geom::{Expanse, Frame, Rect},
};

/// The Layout struct provides all operations that a node can perform during its layout phase.
pub struct Layout {}

impl Layout {
    /// Hides the element and all its descendants from rendering. The nodes are still included in
    /// the tree.
    pub fn hide(&self, child: &mut dyn Node) {
        child.state_mut().hidden = true;
    }

    /// Unhides the element and all its descendants, allowing them to be rendered again.
    pub fn unhide(&self, child: &mut dyn Node) {
        child.state_mut().hidden = false;
    }

    /// Wrap a single child node, mirroring the child's size and view.
    ///
    /// When implementing a simple container that merely exposes its child's
    /// viewport, prefer this over [`fit`]. Calling `fit` recursively from a
    /// widget's `layout` method can lead to unbounded recursion and a stack
    /// overflow.
    ///
    /// Will be deprecated
    pub fn wrap(&self, parent: &mut dyn Node, vp: ViewPort) -> Result<()> {
        // Mirror the child's size and view
        parent.state_mut().set_canvas(vp.canvas());
        parent.state_mut().set_view(vp.view());
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

    /// Lay the child out and place it in a given sub-rectangle of a parent's view.
    pub fn place_(&self, child: &mut dyn Node, loc: Rect) -> Result<()> {
        child.layout(self, loc.into())?;
        child.state_mut().set_position(loc.tl);
        Ok(())
    }

    /// Place a child in a given sub-rectangle of a parent's view.
    pub fn place(&self, child: &mut dyn Node, parent_vp: ViewPort, loc: Rect) -> Result<()> {
        child.state_mut().set_position(
            parent_vp
                .position()
                .scroll(loc.tl.x as i16, loc.tl.y as i16),
        );
        child.layout(self, loc.expanse())?;
        child.state_mut().constrain(parent_vp);
        Ok(())
    }

    pub fn size(&self, n: &mut dyn Node, sz: Expanse, view_size: Expanse) -> Result<()> {
        n.state_mut().fit_size(sz, view_size);
        Ok(())
    }

    /// Set the view rectangle for a node.
    pub fn set_view(&self, node: &mut dyn Node, view: Rect) {
        node.state_mut().set_view(view);
    }

    /// Constrain a child node within its parent's viewport.
    pub fn constrain_child(&self, child: &mut dyn Node, parent_vp: ViewPort) {
        child.state_mut().constrain(parent_vp);
    }

    /// Adjust a child node so that it fits a viewport. This lays the node out to
    /// the viewport's screen rectangle, then adjusts the node's view to place as
    /// much of it within the viewport's screen rectangle as possible.
    ///
    /// Note that [`fit`] will call the child's [`Node::layout`] method. Calling
    /// `fit` on a node from within its own `layout` implementation will recurse
    /// endlessly.
    pub fn fit(&self, n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
        {
            let st = n.state_mut();
            if st.in_layout {
                return Err(crate::error::Error::Layout(
                    "recursive Layout::fit call".into(),
                ));
            }
            st.in_layout = true;
        }
        let res = n.layout(self, parent_vp.screen_rect().into());
        n.state_mut().in_layout = false;
        res?;
        n.state_mut().set_position(parent_vp.position());
        n.state_mut().constrain(parent_vp);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue};
    use crate::state::NodeName;
    use crate::{
        Context, Node, NodeState, Render, Result, StatefulNode,
        geom::{Expanse, Rect},
    };

    // Simple fixed-size test node
    struct TFixed {
        state: NodeState,
        width: u16,
        height: u16,
    }

    impl TFixed {
        fn new(width: u16, height: u16) -> Self {
            TFixed {
                state: NodeState::default(),
                width,
                height,
            }
        }
    }

    impl StatefulNode for TFixed {
        fn name(&self) -> NodeName {
            NodeName::convert("tfixed")
        }

        fn state(&self) -> &NodeState {
            &self.state
        }

        fn state_mut(&mut self) -> &mut NodeState {
            &mut self.state
        }
    }

    impl CommandNode for TFixed {
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

    impl Node for TFixed {
        fn layout(&mut self, l: &Layout, _: Expanse) -> Result<()> {
            let w = self.width;
            let h = self.height;
            l.fill(self, Expanse::new(w, h))
        }

        fn render(&mut self, _: &dyn Context, _: &mut Render) -> Result<()> {
            Ok(())
        }
    }

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

    #[test]
    #[ignore = "This test is not fixed yet"]
    fn frame_does_not_overflow_small_parent() -> Result<()> {
        let l = Layout {};
        let mut child = TFixed::new(2, 2);
        assert!(l.frame(&mut child, Expanse::new(1, 1), 1).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "This test is not fixed yet"]
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
        assert!(l.frame(&mut n, Expanse::new(0, 0), 1).is_err());

        Ok(())
    }
}
