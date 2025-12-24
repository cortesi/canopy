use std::process;

use slotmap::Key;
use taffy::style::Style;

use crate::{
    Context, ViewContext,
    core::{NodeId, viewport::ViewPort},
    error::Result,
    geom::{Direction, Expanse, Rect},
    path::Path,
    widget::Widget,
};

/// Dummy context for tests.
pub struct DummyContext {
    /// Current node identifier.
    node_id: NodeId,
    /// Root node identifier.
    root_id: NodeId,
}

impl Default for DummyContext {
    fn default() -> Self {
        Self {
            node_id: NodeId::null(),
            root_id: NodeId::null(),
        }
    }
}

impl ViewContext for DummyContext {
    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.root_id
    }

    fn viewport(&self) -> Rect {
        Rect::zero()
    }

    fn view(&self) -> Rect {
        Rect::zero()
    }

    fn canvas(&self) -> Expanse {
        Expanse::new(0, 0)
    }

    fn node_viewport(&self, _node: NodeId) -> Option<Rect> {
        None
    }

    fn node_view(&self, _node: NodeId) -> Option<Rect> {
        None
    }

    fn node_canvas(&self, _node: NodeId) -> Option<Expanse> {
        None
    }

    fn node_vp(&self, _node: NodeId) -> Option<ViewPort> {
        None
    }

    fn children(&self, _node: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    fn is_focused(&self) -> bool {
        false
    }

    fn node_is_focused(&self, _node: NodeId) -> bool {
        false
    }

    fn is_on_focus_path(&self) -> bool {
        false
    }

    fn node_is_on_focus_path(&self, _node: NodeId) -> bool {
        false
    }

    fn focus_path(&self, _root: NodeId) -> Path {
        Path::empty()
    }
}

impl Context for DummyContext {
    fn set_focus(&mut self, _node: NodeId) -> bool {
        false
    }

    fn focus_dir(&mut self, _root: NodeId, _dir: Direction) {}

    fn focus_first(&mut self, _root: NodeId) {}

    fn focus_next(&mut self, _root: NodeId) {}

    fn focus_prev(&mut self, _root: NodeId) {}

    fn scroll_to(&mut self, _x: u32, _y: u32) -> bool {
        false
    }

    fn scroll_by(&mut self, _x: i32, _y: i32) -> bool {
        false
    }

    fn page_up(&mut self) -> bool {
        false
    }

    fn page_down(&mut self) -> bool {
        false
    }

    fn scroll_up(&mut self) -> bool {
        false
    }

    fn scroll_down(&mut self) -> bool {
        false
    }

    fn scroll_left(&mut self) -> bool {
        false
    }

    fn scroll_right(&mut self) -> bool {
        false
    }

    fn with_style(&mut self, _node: NodeId, _f: &mut dyn FnMut(&mut Style)) -> Result<()> {
        Ok(())
    }

    fn add(&mut self, _widget: Box<dyn Widget>) -> NodeId {
        NodeId::null()
    }

    fn with_widget_mut(
        &mut self,
        _node: NodeId,
        _f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }

    fn mount_child(&mut self, _parent: NodeId, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn detach_child(&mut self, _parent: NodeId, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn set_children(&mut self, _parent: NodeId, _children: Vec<NodeId>) -> Result<()> {
        Ok(())
    }

    fn set_hidden(&mut self, _node: NodeId, _hidden: bool) -> bool {
        false
    }

    fn start(&mut self) -> Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn exit(&mut self, code: i32) -> ! {
        process::exit(code)
    }
}
