//! Helper functions for `Node::layout` implementations.

use crate::{
    Node, Result, ViewPort,
    geom::{Expanse, Rect},
};

/// The Layout struct provides operations that a node can perform on children during its layout
/// phase.
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

    /// Fill a node to occupy the given size, resetting its view to start at (0,0).
    /// This is typically used for root nodes or nodes that should always show
    /// their content from the top-left corner.
    pub fn fill(&self, n: &mut dyn Node, sz: Expanse) -> Result<()> {
        n.state_mut().set_canvas(sz);
        n.state_mut().set_view(sz.rect());
        Ok(())
    }

    /// Fill a node to occupy the given size while preserving its current scroll position.
    /// This is useful for nodes that need to maintain their viewport position across
    /// layout updates (e.g., scrollable content areas).
    pub fn fill_preserve_scroll(&self, n: &mut dyn Node, sz: Expanse) -> Result<()> {
        n.state_mut().fit_size(sz, sz);
        Ok(())
    }

    /// Lay the child out and place it in a given sub-rectangle of a parent's canvas.
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

    /// Adjust a child node so that it fits a viewport. This lays the node out to the parent's view
    /// rectangle, then adjusts the node's position to match.
    // FIXME: this shoudl just take a rect
    pub fn fit(&self, n: &mut dyn Node, parent_vp: ViewPort) -> Result<()> {
        n.layout(self, parent_vp.view().into())?;
        n.state_mut().set_position(parent_vp.position());
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
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 20, 20), (10, 10))?
        );

        // The child can shift its view freely
        n.state_mut().scroll_right();
        n.state_mut().scroll_down();
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 20, 20), (10, 10))?
        );

        // Subsequent fits reset the child view position
        l.fit(&mut n, vp)?;
        assert_eq!(
            n.state().viewport,
            ViewPort::new(Expanse::new(20, 20), Rect::new(0, 0, 20, 20), (10, 10))?
        );

        Ok(())
    }
}
