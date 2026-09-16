use std::{any::TypeId, result::Result as StdResult};

use crate::{
    ChangeOutcome, Context, FocusDirection, FocusScope, ViewContext,
    commands::{
        ArgValue, CommandError, CommandInvocation, CommandScopeFrame, CommandStatus, CommandTarget,
        ListRowContext,
    },
    core::{
        NodeId, context::sealed, help::BindingSnapshot, id::testing_node_id,
        style::effects::Effect, view::View,
    },
    error::{Error, Result},
    event::{Event, mouse::MouseEvent},
    geom::{Point, Rect},
    layout::{Layout, LayoutOverride},
    path::Path,
    style::StyleMap,
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
            node_id: testing_node_id(),
            root_id: testing_node_id(),
        }
    }
}

impl ViewContext for DummyContext {
    fn has_mouse_capture(&self) -> bool {
        false
    }

    fn find_identity(&self, _scope: NodeId, _key: &str) -> Result<Option<NodeId>> {
        Ok(None)
    }
    fn semantic_identity(&self, _node: NodeId) -> Option<crate::SemanticIdentity> {
        None
    }

    fn node_id(&self) -> NodeId {
        self.node_id
    }

    fn root_id(&self) -> NodeId {
        self.root_id
    }

    fn view_of(&self, _node: NodeId) -> Option<View> {
        None
    }

    fn layout_of(&self, _node: NodeId) -> Option<Layout> {
        None
    }

    fn with_widget_dyn(
        &self,
        node: NodeId,
        _callback: &mut dyn FnMut(&dyn Widget) -> Result<()>,
    ) -> Result<()> {
        Err(Error::NodeNotFound(node))
    }
    fn command_status(
        &self,
        _target: CommandTarget,
        _invocation: &CommandInvocation,
    ) -> Result<CommandStatus> {
        Ok(CommandStatus::Enabled)
    }
    fn type_id_of(&self, _node: NodeId) -> Option<TypeId> {
        None
    }

    fn children_of(&self, _node: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    fn is_focused_of(&self, _node: NodeId) -> bool {
        false
    }

    fn focused_node(&self) -> Option<NodeId> {
        None
    }

    fn is_on_focus_path_of(&self, _node: NodeId) -> bool {
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

    fn is_attached_of(&self, _node: NodeId) -> bool {
        false
    }

    fn path_of(&self, _root: NodeId, _node: NodeId) -> Path {
        Path::empty()
    }

    fn locate(&self, _root: NodeId, _point: Point) -> Result<Option<NodeId>> {
        Ok(None)
    }

    fn child_slot_of(&self, _parent: NodeId, _key: &str) -> Option<NodeId> {
        None
    }
}

impl sealed::ViewContext for DummyContext {}

impl sealed::Context for DummyContext {
    fn as_context(&mut self) -> &mut dyn Context {
        self
    }

    fn attach_composed(
        &mut self,
        _parent: NodeId,
        _roots: &[(NodeId, Option<&str>)],
        _slots: &[(NodeId, NodeId, String)],
    ) -> Result<()> {
        Ok(())
    }
}

impl Context for DummyContext {
    fn set_semantic_key(&mut self, _node: NodeId, _scope: NodeId, _key: &str) -> Result<()> {
        Ok(())
    }
    fn clear_semantic_key(&mut self, _node: NodeId) -> Result<()> {
        Ok(())
    }

    fn set_focus(&mut self, _node: NodeId) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_first(&mut self, _scope: FocusScope) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn focus_move(
        &mut self,
        _scope: FocusScope,
        _direction: FocusDirection,
    ) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn capture_mouse(&mut self) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn release_mouse(&mut self) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn take_mouse_capture(&mut self) -> Result<Option<NodeId>> {
        Ok(None)
    }

    fn available_bindings(&self, _node: Option<NodeId>) -> Result<BindingSnapshot> {
        Ok(BindingSnapshot {
            focus: self.root_id,
            focus_path: Path::empty(),
            active_modes: Vec::new(),
            transient_mode: None,
            exclusive_group: None,
            bindings: Vec::new(),
            mouse_bindings: Vec::new(),
        })
    }

    fn scroll_to(&mut self, _x: u32, _y: u32) -> ChangeOutcome {
        ChangeOutcome::Unchanged
    }

    fn scroll_by(&mut self, _x: i32, _y: i32) -> ChangeOutcome {
        ChangeOutcome::Unchanged
    }

    fn scroll_to_of(&mut self, _node: NodeId, _x: u32, _y: u32) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn reveal_area(&mut self, _area: Rect, _align: crate::RevealAlign) -> ChangeOutcome {
        ChangeOutcome::Unchanged
    }

    fn reveal_anchor(&mut self, _align: crate::RevealAlign) -> ChangeOutcome {
        ChangeOutcome::Unchanged
    }

    fn reveal_node(&mut self, _node: NodeId, _align: crate::RevealAlign) -> Result<ChangeOutcome> {
        Ok(ChangeOutcome::Unchanged)
    }

    fn invalidate_layout(&mut self) {}

    fn set_layout_override_of(&mut self, _node: NodeId, _overrides: LayoutOverride) -> Result<()> {
        Ok(())
    }

    fn with_layout_of(&mut self, _node: NodeId, _f: &mut dyn FnMut(&mut Layout)) -> Result<()> {
        Ok(())
    }

    fn create_detached_boxed(&mut self, _widget: Box<dyn Widget>) -> Result<NodeId> {
        Ok(testing_node_id())
    }

    fn edit_structure(
        &mut self,
        edit: &mut dyn FnMut(&mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        edit(self)
    }

    fn with_widget_dyn_mut(
        &mut self,
        _node: NodeId,
        _f: &mut dyn FnMut(&mut dyn Widget, &mut dyn Context) -> Result<()>,
    ) -> Result<()> {
        Ok(())
    }

    fn dispatch(
        &mut self,
        _target: CommandTarget,
        _cmd: &CommandInvocation,
    ) -> StdResult<ArgValue, CommandError> {
        Ok(ArgValue::Null)
    }
    fn dispatch_scoped(
        &mut self,
        _target: CommandTarget,
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
        Ok(testing_node_id())
    }

    fn add_child_to_slot_boxed(
        &mut self,
        _parent: NodeId,
        _key: &str,
        _widget: Box<dyn Widget>,
    ) -> Result<NodeId> {
        Ok(testing_node_id())
    }

    fn attach(&mut self, _parent: NodeId, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn attach_slot(&mut self, _parent: NodeId, _key: &str, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn detach(&mut self, _child: NodeId) -> Result<()> {
        Ok(())
    }

    fn wake_handle(&self, _lifetime: crate::WorkLifetime) -> Result<crate::NodeWakeHandle> {
        Err(Error::NodeNotFound(self.node_id))
    }

    fn remove_after_dispatch(&mut self, _node: NodeId) -> Result<()> {
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

    fn set_style(&mut self, _style: StyleMap) {
        // DummyContext does not track styles
    }

    fn request_diagnostic_dump(&mut self, _target: NodeId) {
        // DummyContext does not track diagnostic requests
    }
}
