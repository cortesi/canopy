use std::{any::TypeId, process};

use slotmap::Key;

use crate::{
    Context, ViewContext,
    commands::{CommandInvocation, ReturnValue},
    core::{NodeId, style::StyleEffect, view::View},
    error::Result,
    geom::{Direction, Expanse, Point, PointI32, RectI32},
    layout::Layout,
    path::Path,
    style::StyleMap,
    widget::Widget,
};

/// Default view used by DummyContext.
const DUMMY_VIEW: View = View {
    outer: RectI32 {
        tl: PointI32 { x: 0, y: 0 },
        w: 0,
        h: 0,
    },
    content: RectI32 {
        tl: PointI32 { x: 0, y: 0 },
        w: 0,
        h: 0,
    },
    tl: Point { x: 0, y: 0 },
    canvas: Expanse { w: 0, h: 0 },
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

    fn view(&self) -> &View {
        &DUMMY_VIEW
    }

    fn layout(&self) -> Layout {
        Layout::default()
    }

    fn node_view(&self, _node: NodeId) -> Option<View> {
        None
    }

    fn node_type_id(&self, _node: NodeId) -> Option<TypeId> {
        None
    }

    fn invalidate_layout(&self) {}

    fn children_of(&self, _node: NodeId) -> Vec<NodeId> {
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

    fn focused_leaf(&self, _root: NodeId) -> Option<NodeId> {
        None
    }

    fn focusable_leaves(&self, _root: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    fn parent_of(&self, _node: NodeId) -> Option<NodeId> {
        None
    }

    fn node_path(&self, _root: NodeId, _node: NodeId) -> Path {
        Path::empty()
    }
}

impl Context for DummyContext {
    fn set_focus(&mut self, _node: NodeId) -> bool {
        false
    }

    fn focus_dir_in(&mut self, _root: NodeId, _dir: Direction) {}

    fn focus_first_in(&mut self, _root: NodeId) {}

    fn focus_next_in(&mut self, _root: NodeId) {}

    fn focus_prev_in(&mut self, _root: NodeId) {}

    fn capture_mouse(&mut self) -> bool {
        false
    }

    fn release_mouse(&mut self) -> bool {
        false
    }

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

    fn with_layout_of(&mut self, _node: NodeId, _f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        Ok(())
    }

    fn create_detached_boxed(&mut self, _widget: Box<dyn Widget>) -> NodeId {
        NodeId::null()
    }

    fn with_widget_mut(
        &mut self,
        _node: NodeId,
        _f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }

    fn dispatch_command(&mut self, _cmd: &CommandInvocation) -> Result<Option<ReturnValue>> {
        Ok(None)
    }

    fn add_child_to_boxed(&mut self, _parent: NodeId, _widget: Box<dyn Widget>) -> Result<NodeId> {
        Ok(NodeId::null())
    }

    fn add_child_to_keyed_boxed(
        &mut self,
        _parent: NodeId,
        _key: &str,
        _widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        Ok(NodeId::null())
    }

    fn attach(&mut self, _parent: NodeId, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn attach_keyed(&mut self, _parent: NodeId, _key: &str, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn detach(&mut self, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn remove_subtree(&mut self, _node: NodeId) -> Result<()> {
        Ok(())
    }

    fn child_keyed(&self, _key: &str) -> Option<NodeId> {
        None
    }

    fn set_children_of(&mut self, _parent: NodeId, _children: Vec<NodeId>) -> Result<()> {
        Ok(())
    }

    fn set_hidden_of(&mut self, _node: NodeId, _hidden: bool) -> bool {
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

    fn push_effect(&mut self, _node: NodeId, _effect: Box<dyn StyleEffect>) -> Result<()> {
        Ok(())
    }

    fn clear_effects(&mut self, _node: NodeId) -> Result<()> {
        Ok(())
    }

    fn set_clear_inherited_effects(&mut self, _node: NodeId, _clear: bool) -> Result<()> {
        Ok(())
    }

    fn set_style(&mut self, _style: StyleMap) {
        // DummyContext does not track styles
    }
}
