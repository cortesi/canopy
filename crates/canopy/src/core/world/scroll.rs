//! Viewport scroll mutation and default input actions.

use super::{Core, layout_driver::clamp_scroll};
use crate::{
    ChangeOutcome,
    core::id::NodeId,
    error::Result,
    geom::{Point, PointI32},
};

/// Runtime behavior applied to a route node that no widget or binding handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// Scroll the node's viewport by a signed offset.
    Scroll(PointI32),
}

impl Core {
    /// Scroll an attached node to an offset, clamped to its canvas.
    pub(crate) fn scroll_to_of(&mut self, node: NodeId, x: u32, y: u32) -> Result<ChangeOutcome> {
        self.validate_attached_node(node)?;
        Ok(self.update_scroll(node, |_| Point { x, y }))
    }

    /// Apply a scroll transform to a node, clamp it to the canvas, and cancel
    /// the node's pending reveal.
    ///
    /// Reports a change when the offset moved or a pending reveal was
    /// cancelled.
    pub(crate) fn update_scroll(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(Point) -> Point,
    ) -> ChangeOutcome {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return ChangeOutcome::Unchanged;
        };
        let before = node.scroll;
        let cancelled_reveal = node.pending_reveal.take().is_some();
        node.scroll = f(before);
        clamp_scroll(&mut node.scroll, node.content_size, node.canvas);
        node.view.scroll = node.scroll;
        if before == node.scroll && !cancelled_reveal {
            return ChangeOutcome::Unchanged;
        }
        self.invalidate(crate::Invalidation::Layout);
        ChangeOutcome::Changed
    }

    /// Apply a default action to a node and report whether the node moved.
    ///
    /// A step that cannot move leaves the node untouched, so pending reveals
    /// survive it.
    pub(crate) fn apply_default_action(&mut self, node_id: NodeId, action: DefaultAction) -> bool {
        match action {
            DefaultAction::Scroll(delta) => {
                let Some(node) = self.nodes.get(node_id) else {
                    return false;
                };
                let mut target = node.scroll.scroll(delta.x, delta.y);
                clamp_scroll(&mut target, node.content_size, node.canvas);
                if target == node.scroll {
                    return false;
                }
                self.update_scroll(node_id, |_| target);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::test_support::*, *};
    use crate::{
        Context, ViewContext,
        core::context::CoreContext,
        error::Error,
        geom::{Rect, Size},
        layout::{Layout, Measurement},
    };

    /// Attach a fill-sized surface whose canvas is 100 cells on each axis.
    fn scroll_surface(core: &mut Core) -> Result<NodeId> {
        let (widget, _) =
            TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(100, 100));
        let child = core.create_detached(widget)?;
        attach_root_child(core, child)?;
        core.set_layout_of(child, Layout::fill())?;
        Ok(child)
    }

    #[test]
    fn default_scrolling_moves_each_axis_and_stops_at_the_canvas_edges() -> Result<()> {
        let mut core = Core::new();
        let child = scroll_surface(&mut core)?;
        core.update_layout(Size::new(10, 4))?;
        let step = |core: &mut Core, x, y| {
            core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x, y }))
        };

        assert!(!step(&mut core, 0, -3), "the top edge cannot move up");
        assert!(!step(&mut core, -3, 0), "the left edge cannot move left");
        assert!(step(&mut core, 0, 3));
        assert!(step(&mut core, 3, 0));
        assert_eq!(core.nodes[child].scroll, Point { x: 3, y: 3 });
        assert!(step(&mut core, 0, i32::MAX));
        assert_eq!(core.nodes[child].scroll, Point { x: 3, y: 96 });
        assert!(!step(&mut core, 0, 3), "the bottom edge cannot move down");
        assert!(step(&mut core, i32::MIN, 0));
        assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 96 });
        Ok(())
    }

    #[test]
    fn an_immovable_default_step_preserves_a_pending_reveal() -> Result<()> {
        let mut core = Core::new();
        let child = scroll_surface(&mut core)?;
        core.update_layout(Size::new(10, 4))?;

        CoreContext::new(&mut core, child).scroll_into_view(Rect::new(0, 40, 1, 1));
        assert!(!core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x: 0, y: -3 })));
        core.update_layout(Size::new(10, 4))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 37 });

        CoreContext::new(&mut core, child).scroll_into_view(Rect::new(0, 80, 1, 1));
        assert!(core.apply_default_action(child, DefaultAction::Scroll(PointI32 { x: 0, y: 3 })));
        core.update_layout(Size::new(10, 4))?;
        assert_eq!(
            core.nodes[child].scroll,
            Point { x: 0, y: 40 },
            "a step that moves supersedes the older reveal"
        );
        Ok(())
    }

    #[test]
    fn scrolling_another_node_rejects_missing_and_detached_targets() -> Result<()> {
        let mut core = Core::new();
        let child = scroll_surface(&mut core)?;
        let detached = core.create_detached(simple_widget())?;
        core.update_layout(Size::new(10, 4))?;
        let root = core.root;
        let mut ctx = CoreContext::new(&mut core, root);

        assert!(ctx.scroll_to_of(child, 5, 200)?.changed());
        assert_eq!(
            ctx.view_of(child).map(|view| view.scroll),
            Some(Point { x: 5, y: 96 })
        );
        assert!(!ctx.scroll_to_of(child, 5, 96)?.changed());
        assert!(matches!(
            ctx.scroll_to_of(detached, 1, 1),
            Err(Error::NodeDetached(_))
        ));
        core.remove_subtree(detached)?;
        assert!(matches!(
            CoreContext::new(&mut core, root).scroll_to_of(detached, 1, 1),
            Err(Error::NodeNotFound(_))
        ));
        Ok(())
    }
}
