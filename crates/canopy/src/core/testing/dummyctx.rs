use std::{any::TypeId, result::Result as StdResult};

use slotmap::Key;

use crate::{
    ChangeOutcome, Context, FocusScope, ViewContext,
    commands::{ArgValue, CommandError, CommandInvocation, CommandScopeFrame, ListRowContext},
    core::{NodeId, help::OwnedHelpSnapshot, style::Effect, view::View},
    error::Result,
    event::{Event, mouse::MouseEvent},
    geom::{Direction, Point, PointI32, RectI32, Size},
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
    canvas: Size { w: 0, h: 0 },
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

    fn node_layout(&self, _node: NodeId) -> Option<Layout> {
        None
    }

    fn node_type_id(&self, _node: NodeId) -> Option<TypeId> {
        None
    }

    fn children_of(&self, _node: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    fn is_focused(&self) -> bool {
        false
    }

    fn node_is_focused(&self, _node: NodeId) -> bool {
        false
    }

    fn focused_node(&self) -> Option<NodeId> {
        None
    }

    fn is_on_focus_path(&self) -> bool {
        false
    }

    fn node_is_on_focus_path(&self, _node: NodeId) -> bool {
        false
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

    fn node_is_attached(&self, _node: NodeId) -> bool {
        false
    }

    fn node_path(&self, _root: NodeId, _node: NodeId) -> Path {
        Path::empty()
    }

    fn locate(&self, _root: NodeId, _point: Point) -> Result<Option<NodeId>> {
        Ok(None)
    }

    fn child_keyed(&self, _key: &str) -> Option<NodeId> {
        None
    }

    fn child_keyed_in(&self, _parent: NodeId, _key: &str) -> Option<NodeId> {
        None
    }

    fn pending_help_snapshot(&self) -> Option<&OwnedHelpSnapshot> {
        None
    }
}

impl Context for DummyContext {
    fn set_focus(&mut self, _node: NodeId) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_dir(&mut self, _scope: FocusScope, _dir: Direction) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_first(&mut self, _scope: FocusScope) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_next(&mut self, _scope: FocusScope) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_prev(&mut self, _scope: FocusScope) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn capture_mouse(&mut self) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn release_mouse(&mut self) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn scroll_to(&mut self, _x: u32, _y: u32) -> bool {
        false
    }

    fn scroll_by(&mut self, _x: i32, _y: i32) -> bool {
        false
    }

    fn invalidate_layout(&mut self) {}

    fn with_layout_of(&mut self, _node: NodeId, _f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        Ok(())
    }

    fn create_detached_boxed(&mut self, _widget: Box<dyn Widget>) -> Result<NodeId> {
        Ok(NodeId::null())
    }

    fn apply_tree_edit(
        &mut self,
        edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        edit(self)
    }

    fn with_widget_mut(
        &mut self,
        _node: NodeId,
        _f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }

    fn dispatch_command(&mut self, _cmd: &CommandInvocation) -> StdResult<ArgValue, CommandError> {
        Ok(ArgValue::Null)
    }

    fn dispatch_command_scoped(
        &mut self,
        _frame: CommandScopeFrame,
        _cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        Ok(ArgValue::Null)
    }

    fn current_event(&self) -> Option<&Event> {
        None
    }

    fn current_mouse_event(&self) -> Option<MouseEvent> {
        None
    }

    fn current_list_row(&self) -> Option<ListRowContext> {
        None
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

    fn set_children_of(&mut self, _parent: NodeId, _children: Vec<NodeId>) -> Result<()> {
        Ok(())
    }

    fn set_hidden_of(&mut self, _node: NodeId, _hidden: bool) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn exit(&mut self, _code: i32) {}

    fn push_effect(&mut self, _node: NodeId, _effect: Effect) -> Result<()> {
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

    fn request_help_snapshot(&mut self, _target: NodeId) {
        // DummyContext does not track help requests
    }

    fn take_help_snapshot(&mut self) -> Option<OwnedHelpSnapshot> {
        // DummyContext does not track help snapshots
        None
    }

    fn request_diagnostic_dump(&mut self, _target: NodeId) {
        // DummyContext does not track diagnostic requests
    }
}
