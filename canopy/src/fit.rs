use std::collections::HashMap;

use super::*;
use crate::geom::{Expanse, Point, Rect};

struct NodeLayout {
    position: Point,
    view: Rect,
    canvas: Expanse,
}

impl NodeLayout {
    fn new(canvas: Expanse, view: Rect) -> Self {
        NodeLayout {
            position: Point { x: 0, y: 0 },
            view,
            canvas,
        }
    }
}

pub struct Layout {
    viewports: HashMap<NodeId, NodeLayout>,
    parent_child: HashMap<NodeId, NodeId>,
    child_parent: HashMap<NodeId, NodeId>,
}

impl Layout {
    /// Position a child node within the parent's canvas. The child must already have a view. The parent may or may not
    /// have a view - if it does we validate that the position lies within the parent's view. A node can only be
    /// positioned once in a render sweep - placing it a second time is an error. By default, all nodes are positioned
    /// at (0, 0), so calling this method is only necessary if you want to place a node at a different position.
    pub fn set_position<'a, P, C>(&mut self, parent: P, child: C, p: Point) -> Result<()>
    where
        P: Into<&'a NodeId>,
        C: Into<&'a NodeId>,
    {
        let pid = parent.into();
        let cid = child.into();
        self.child_parent.insert(cid.clone(), pid.clone());
        self.parent_child.insert(pid.clone(), cid.clone());
        if let Some(vp) = self.viewports.get_mut(&pid) {
            vp.position = p;
        } else {
            return Err(Error::Layout(format!(
                "Parent node {} has no viewport",
                pid
            )));
        }
        Ok(())
    }

    /// Set the viewport of a node.
    pub fn set_viewport<T: Into<NodeId>>(&mut self, node: T, canvas: Expanse, view: Rect) {}

    /// Set the size of a node to wrap all its children. All children must be positioned and have views.
    pub fn wrap_children<T: Into<NodeId>>(&mut self, node: T, target: Expanse) -> Result<Expanse> {
        Ok(target)
    }

    /// Set the viewport of a node to wrap a child node, and return the resulting canvas size.
    pub fn view_wrap(
        &mut self,
        parent: &dyn Node,
        child: &dyn Node,
        canvas: Expanse,
    ) -> Result<Expanse> {
        Ok(canvas)
    }
}
