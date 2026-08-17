use std::{
    any::TypeId,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

use proptest::{prelude::*, test_runner::TestCaseResult};

use super::test_support::{TestWidget, assert_error_context};
use super::{layout_driver::refresh_layouts, *};
use crate::{
    Context, KeyedChildren, RemovePolicy,
    core::{
        context::{CoreContext, CoreViewContext},
        script::validate_node_handle,
        testing::model::trace_result,
    },
    error::{Error, NodeOperationKind, Result},
    geom::Size,
    layout::{Layout, Measurement},
    path::Path,
    widget::Widget,
};

struct FocusableWidget;

impl Widget for FocusableWidget {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }
}

struct MountFailWidget;

impl Widget for MountFailWidget {
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Err(Error::Invalid("mount failed".into()))
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StructuralSnapshot {
    root: NodeId,
    nodes: Vec<NodeSnapshot>,
    focus: Option<NodeId>,
    mouse_capture: Option<NodeId>,
    focus_hint: Option<(Option<NodeId>, Option<NodeId>, Option<NodeId>)>,
    exit_requested: Option<i32>,
    pending_style: bool,
    commands: Vec<&'static str>,
    pending_help_request: Option<(NodeId, Option<NodeId>)>,
    pending_help_snapshot: bool,
    pending_help_snapshot_observed: bool,
    pending_diagnostic_dump: Option<NodeId>,
}

#[derive(Debug, PartialEq, Eq)]
struct NodeSnapshot {
    id: NodeId,
    widget_type: TypeId,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    child_keys: Vec<(String, NodeId)>,
    hidden: bool,
    initialized: bool,
    mounted: bool,
}

impl StructuralSnapshot {
    fn capture(core: &Core) -> Self {
        let nodes = core
            .nodes
            .iter()
            .map(|(id, node)| {
                let mut child_keys: Vec<_> = node
                    .child_keys
                    .iter()
                    .map(|(key, child)| (key.clone(), *child))
                    .collect();
                child_keys.sort_by(|left, right| left.0.cmp(&right.0));
                NodeSnapshot {
                    id,
                    widget_type: node.widget_type,
                    parent: node.parent,
                    children: node.children.clone(),
                    child_keys,
                    hidden: node.hidden,
                    initialized: node.initialized,
                    mounted: node.mounted,
                }
            })
            .collect();
        let focus_hint = core
            .focus_hint
            .map(|hint| (hint.next, hint.prev, hint.ancestor));
        let mut commands: Vec<_> = core.commands.iter().map(|(id, _)| id).collect();
        commands.sort_unstable();
        Self {
            root: core.root,
            nodes,
            focus: core.focus,
            mouse_capture: core.mouse_capture,
            focus_hint,
            exit_requested: core.exit_requested,
            pending_style: core.pending_style.is_some(),
            commands,
            pending_help_request: core.pending_help_request,
            pending_help_snapshot: core.pending_help_snapshot.is_some(),
            pending_help_snapshot_observed: core.pending_help_snapshot_observed.get(),
            pending_diagnostic_dump: core.pending_diagnostic_dump,
        }
    }
}

fn assert_structural_rollback(
    core: &mut Core,
    edit: impl FnOnce(&mut Core) -> Result<()>,
) -> Error {
    let before = StructuralSnapshot::capture(core);
    let error = edit(core).expect_err("fault-injected edit should fail");
    assert_eq!(StructuralSnapshot::capture(core), before);
    core.validate_invariants()
        .expect("rolled-back core should satisfy its invariants");
    error
}

#[derive(Clone, Copy)]
enum MountAction {
    Succeed,
    FailAfterCoreMutations,
    FailFromNestedEdit,
    HandleNestedFailure,
}

#[derive(Clone, Copy)]
enum PreRemoveAction {
    Succeed,
    Fail,
    RemoveThenFail(NodeId),
}

#[derive(Clone, Copy)]
enum UnmountAction {
    None,
    TryStructuralEdit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HookEvent {
    Mount(&'static str),
    PreRemove(&'static str),
    Unmount(&'static str),
    RollbackEditRejected(bool),
}

struct FaultWidget {
    name: &'static str,
    log: Arc<Mutex<Vec<HookEvent>>>,
    mount: MountAction,
    pre_remove: PreRemoveAction,
    unmount: UnmountAction,
}

impl FaultWidget {
    fn new(name: &'static str, log: Arc<Mutex<Vec<HookEvent>>>) -> Self {
        Self {
            name,
            log,
            mount: MountAction::Succeed,
            pre_remove: PreRemoveAction::Succeed,
            unmount: UnmountAction::None,
        }
    }

    fn with_mount(mut self, mount: MountAction) -> Self {
        self.mount = mount;
        self
    }

    fn with_pre_remove(mut self, pre_remove: PreRemoveAction) -> Self {
        self.pre_remove = pre_remove;
        self
    }

    fn with_unmount(mut self, unmount: UnmountAction) -> Self {
        self.unmount = unmount;
        self
    }

    fn record(&self, event: HookEvent) {
        self.log.lock().unwrap().push(event);
    }
}

impl Widget for FaultWidget {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.record(HookEvent::Mount(self.name));
        match self.mount {
            MountAction::Succeed => Ok(()),
            MountAction::FailAfterCoreMutations => {
                ctx.set_hidden(true)?;
                ctx.exit(73);
                ctx.request_diagnostic_dump(ctx.node_id());
                Err(Error::Invalid("fault-injected mount failure".into()))
            }
            MountAction::FailFromNestedEdit => {
                ctx.add_child(MountFailWidget)?;
                Ok(())
            }
            MountAction::HandleNestedFailure => {
                let error = match ctx.add_child(MountFailWidget) {
                    Ok(_) => panic!("nested mount should fail"),
                    Err(error) => error,
                };
                assert!(matches!(error, Error::Invalid(_)));
                Ok(())
            }
        }
    }

    fn pre_remove(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.record(HookEvent::PreRemove(self.name));
        match self.pre_remove {
            PreRemoveAction::Succeed => Ok(()),
            PreRemoveAction::Fail => {
                Err(Error::Invalid("fault-injected pre-remove failure".into()))
            }
            PreRemoveAction::RemoveThenFail(node_id) => {
                ctx.remove_subtree(node_id)?;
                Err(Error::Invalid(
                    "fault-injected nested pre-remove failure".into(),
                ))
            }
        }
    }

    fn on_unmount(&mut self, ctx: &mut dyn Context) {
        self.record(HookEvent::Unmount(self.name));
        if matches!(self.unmount, UnmountAction::TryStructuralEdit) {
            let rejected = matches!(
                ctx.create_detached(MountFailWidget),
                Err(Error::TreeEditDuringRollback { .. })
            );
            self.record(HookEvent::RollbackEditRejected(rejected));
        }
    }
}

#[derive(Debug)]
struct ReconcileWidget {
    fail_mount: bool,
}

impl ReconcileWidget {
    fn succeeds() -> Self {
        Self { fail_mount: false }
    }

    fn fails_mount() -> Self {
        Self { fail_mount: true }
    }
}

impl Widget for ReconcileWidget {
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        if self.fail_mount {
            Err(Error::Invalid("reconcile mount failure".into()))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
enum TreeMutation {
    Attach { parent: usize, child: usize },
    Detach { child: usize },
    SetChildren { parent: usize, children: Vec<usize> },
    Remove { node: usize },
    Replace { target: usize },
}

const PROPERTY_NODE_COUNT: usize = 8;

fn attach_root_child(core: &mut Core, child: NodeId) -> Result<()> {
    core.set_children(core.root, vec![child])
}

fn simple_widget() -> TestWidget {
    TestWidget::new(|_constraints| Measurement::Fixed(Size::new(1, 1))).0
}

#[test]
fn typed_id_conversion_rejects_wrong_type_and_stale_generation() -> Result<()> {
    let mut core = Core::new();
    let original = core.create_detached(simple_widget())?;

    let typed = {
        let context = CoreContext::new(&mut core, original);
        let context: &dyn Context = &context;
        (context as &dyn ViewContext).typed_id::<TestWidget>(original)?
    };
    assert_eq!(NodeId::from(typed), original);

    let wrong_type = {
        let context = CoreContext::new(&mut core, original);
        let context: &dyn Context = &context;
        (context as &dyn ViewContext).typed_id::<FocusableWidget>(original)
    };
    assert!(matches!(wrong_type, Err(Error::NodeTypeMismatch { .. })));

    core.remove_subtree(original)?;
    let replacement = core.create_detached(simple_widget())?;
    assert_ne!(replacement, original);
    let stale = {
        let context = CoreContext::new(&mut core, replacement);
        let context: &dyn Context = &context;
        (context as &dyn ViewContext).typed_id::<TestWidget>(original)
    };
    assert!(matches!(stale, Err(Error::NodeNotFound(id)) if id == original));
    Ok(())
}

struct CallbackMutationNodes {
    parent: NodeId,
    current: NodeId,
    sibling: NodeId,
    focused: NodeId,
    focus_fallback: NodeId,
    mouse_capture: NodeId,
}

fn callback_mutation_tree() -> Result<(Core, CallbackMutationNodes)> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    let current = core.create_detached(simple_widget())?;
    let sibling = core.create_detached(simple_widget())?;
    let focused = core.create_detached(FocusableWidget)?;
    let focus_fallback = core.create_detached(FocusableWidget)?;
    let mouse_capture = core.create_detached(simple_widget())?;
    core.set_children(
        parent,
        vec![current, sibling, focused, focus_fallback, mouse_capture],
    )?;
    attach_root_child(&mut core, parent)?;
    for node in [
        core.root,
        parent,
        current,
        sibling,
        focused,
        focus_fallback,
        mouse_capture,
    ] {
        core.with_layout_of(node, |layout| {
            *layout = Layout::fill();
        })?;
    }
    core.update_layout(Size::new(40, 20))?;
    let nodes = CallbackMutationNodes {
        parent,
        current,
        sibling,
        focused,
        focus_fallback,
        mouse_capture,
    };
    Ok((core, nodes))
}

fn with_callback_context(
    core: &mut Core,
    node: NodeId,
    f: impl FnOnce(&mut dyn Context) -> Result<()>,
) -> Result<Result<()>> {
    let mut f = Some(f);
    core.with_widget_mut(node, |_widget, core| {
        let mut ctx = CoreContext::new(core, node);
        let f = f.take().expect("callback should run once");
        f(&mut ctx)
    })
}

fn with_callback_core(
    core: &mut Core,
    node: NodeId,
    f: impl FnOnce(&mut Core) -> Result<()>,
) -> Result<Result<()>> {
    let mut f = Some(f);
    core.with_widget_mut(node, |_widget, core| {
        let f = f.take().expect("callback should run once");
        f(core)
    })
}

fn property_nodes(core: &mut Core) -> Result<Vec<Option<NodeId>>> {
    (0..PROPERTY_NODE_COUNT)
        .map(|_| core.create_detached(simple_widget()).map(Some))
        .collect()
}

fn target_node(core: &Core, nodes: &[Option<NodeId>], target: usize) -> Option<NodeId> {
    if target == 0 {
        Some(core.root)
    } else {
        nodes.get(target - 1).copied().flatten()
    }
}

fn child_node(nodes: &[Option<NodeId>], child: usize) -> Option<NodeId> {
    nodes.get(child).copied().flatten()
}

fn forget_removed_nodes(nodes: &mut [Option<NodeId>], core: &Core) {
    for node in nodes {
        if node.is_some_and(|node_id| !core.nodes.contains_key(node_id)) {
            *node = None;
        }
    }
}

fn tree_mutation_strategy() -> impl Strategy<Value = Vec<TreeMutation>> {
    prop::collection::vec(
        prop_oneof![
            (0usize..=PROPERTY_NODE_COUNT, 0usize..PROPERTY_NODE_COUNT)
                .prop_map(|(parent, child)| TreeMutation::Attach { parent, child }),
            (0usize..PROPERTY_NODE_COUNT).prop_map(|child| TreeMutation::Detach { child }),
            (
                0usize..=PROPERTY_NODE_COUNT,
                prop::collection::vec(0usize..PROPERTY_NODE_COUNT, 0..=4),
            )
                .prop_map(|(parent, children)| TreeMutation::SetChildren { parent, children }),
            (0usize..PROPERTY_NODE_COUNT).prop_map(|node| TreeMutation::Remove { node }),
            (0usize..=PROPERTY_NODE_COUNT).prop_map(|target| TreeMutation::Replace { target }),
        ],
        1..80,
    )
}

fn apply_tree_mutation(
    core: &mut Core,
    nodes: &mut [Option<NodeId>],
    mutation: &TreeMutation,
) -> TestCaseResult {
    match mutation {
        TreeMutation::Attach { parent, child } => {
            if let (Some(parent_id), Some(child_id)) =
                (target_node(core, nodes, *parent), child_node(nodes, *child))
            {
                drop(core.attach(parent_id, child_id));
            }
        }
        TreeMutation::Detach { child } => {
            if let Some(child_id) = child_node(nodes, *child) {
                drop(core.detach(child_id));
            }
        }
        TreeMutation::SetChildren { parent, children } => {
            if let Some(parent_id) = target_node(core, nodes, *parent) {
                let child_ids = children
                    .iter()
                    .filter_map(|child| child_node(nodes, *child))
                    .collect();
                drop(core.set_children(parent_id, child_ids));
            }
        }
        TreeMutation::Remove { node } => {
            if let Some(node_id) = child_node(nodes, *node) {
                drop(core.remove_subtree(node_id));
            }
        }
        TreeMutation::Replace { target } => {
            if let Some(target_id) = target_node(core, nodes, *target) {
                drop(core.replace_subtree(target_id, simple_widget()));
            }
        }
    }

    forget_removed_nodes(nodes, core);
    let validation = core.validate_invariants();
    prop_assert!(
        validation.is_ok(),
        "after {mutation:?}, validation failed: {validation:?}"
    );
    Ok(())
}

proptest! {
    #[test]
    fn random_tree_mutations_preserve_invariants(mutations in tree_mutation_strategy()) {
        let mut core = Core::new();
        let mut nodes = property_nodes(&mut core)?;
        prop_assert!(core.validate_invariants().is_ok());

        for (index, mutation) in mutations.iter().enumerate() {
            trace_result(
                apply_tree_mutation(&mut core, &mut nodes, mutation),
                &mutations,
                index,
            )?;
        }
    }
}

#[derive(Clone, Debug)]
enum IdentityMutation {
    Attach(usize),
    Detach(usize),
    Remove(usize),
    Replace { node: usize, focusable: bool },
    Focus(usize),
    Capture(usize),
    Visibility { node: usize, hidden: bool },
    CheckRustHandle(usize),
    CheckScriptHandle(usize),
}

#[derive(Clone, Copy, Debug)]
struct IdentityNodeModel {
    id: NodeId,
    live: bool,
    attached: bool,
    focusable: bool,
    hidden: bool,
}

const IDENTITY_NODE_COUNT: usize = 6;

fn identity_mutation_strategy() -> impl Strategy<Value = Vec<IdentityMutation>> {
    prop::collection::vec(
        prop_oneof![
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::Attach),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::Detach),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::Remove),
            (0usize..IDENTITY_NODE_COUNT, any::<bool>())
                .prop_map(|(node, focusable)| IdentityMutation::Replace { node, focusable }),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::Focus),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::Capture),
            (0usize..IDENTITY_NODE_COUNT, any::<bool>())
                .prop_map(|(node, hidden)| IdentityMutation::Visibility { node, hidden }),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::CheckRustHandle),
            (0usize..IDENTITY_NODE_COUNT).prop_map(IdentityMutation::CheckScriptHandle),
        ],
        1..100,
    )
}

fn assert_identity_model(core: &Core, model: &[IdentityNodeModel]) -> TestCaseResult {
    for node in model {
        prop_assert_eq!(core.nodes.contains_key(node.id), node.live);
        if node.live {
            prop_assert_eq!(core.is_attached_to_root(node.id), node.attached);
            prop_assert_eq!(core.nodes[node.id].hidden, node.hidden);
            let expected_type = if node.focusable {
                TypeId::of::<FocusableWidget>()
            } else {
                TypeId::of::<TestWidget>()
            };
            prop_assert_eq!(core.nodes[node.id].widget_type, expected_type);
        }
    }
    for target in [core.focus, core.mouse_capture].into_iter().flatten() {
        let node = model.iter().find(|node| node.id == target);
        prop_assert!(node.is_some_and(|node| node.live && node.attached));
    }
    prop_assert!(core.validate_invariants().is_ok());
    Ok(())
}

fn apply_identity_mutation(
    core: &mut Core,
    model: &mut [IdentityNodeModel],
    mutation: &IdentityMutation,
) -> TestCaseResult {
    let index = match mutation {
        IdentityMutation::Attach(index)
        | IdentityMutation::Detach(index)
        | IdentityMutation::Remove(index)
        | IdentityMutation::Focus(index)
        | IdentityMutation::Capture(index)
        | IdentityMutation::CheckRustHandle(index)
        | IdentityMutation::CheckScriptHandle(index) => *index,
        IdentityMutation::Replace { node, .. } | IdentityMutation::Visibility { node, .. } => *node,
    };
    let state = model[index];

    match *mutation {
        IdentityMutation::Attach(_) => {
            let result = core.attach(core.root, state.id);
            prop_assert_eq!(result.is_ok(), state.live && !state.attached);
            if result.is_ok() {
                model[index].attached = true;
            }
        }
        IdentityMutation::Detach(_) => {
            let result = core.detach(state.id);
            prop_assert_eq!(result.is_ok(), state.live);
            if result.is_ok() {
                model[index].attached = false;
            }
        }
        IdentityMutation::Remove(_) => {
            let result = core.remove_subtree(state.id);
            prop_assert_eq!(result.is_ok(), state.live);
            if result.is_ok() {
                model[index].live = false;
                model[index].attached = false;
            }
        }
        IdentityMutation::Replace { focusable, .. } => {
            let result = if focusable {
                core.replace_subtree(state.id, FocusableWidget)
            } else {
                core.replace_subtree(state.id, simple_widget())
            };
            prop_assert_eq!(result.is_ok(), state.live);
            if result.is_ok() {
                model[index].focusable = focusable;
            }
        }
        IdentityMutation::Focus(_) => match core.set_focus(state.id) {
            Ok(_) => prop_assert!(state.live && state.attached),
            Err(Error::NodeNotFound(id)) => prop_assert!(!state.live && id == state.id),
            Err(Error::NodeDetached(id)) => {
                prop_assert!(state.live && !state.attached && id == state.id)
            }
            Err(error) => prop_assert!(false, "unexpected focus error: {error}"),
        },
        IdentityMutation::Capture(_) => match core.capture_mouse(state.id) {
            Ok(_) => prop_assert!(state.live && state.attached),
            Err(Error::NodeNotFound(id)) => prop_assert!(!state.live && id == state.id),
            Err(Error::NodeDetached(id)) => {
                prop_assert!(state.live && !state.attached && id == state.id)
            }
            Err(error) => prop_assert!(false, "unexpected capture error: {error}"),
        },
        IdentityMutation::Visibility { hidden, .. } => {
            let result = core.set_hidden(state.id, hidden);
            prop_assert_eq!(result.is_ok(), state.live);
            if result.is_ok() {
                model[index].hidden = hidden;
            }
        }
        IdentityMutation::CheckRustHandle(_) => {
            let context = CoreViewContext::new(core, core.root);
            let context: &dyn ViewContext = &context;
            let valid = if state.focusable {
                context.typed_id::<FocusableWidget>(state.id).is_ok()
            } else {
                context.typed_id::<TestWidget>(state.id).is_ok()
            };
            prop_assert_eq!(valid, state.live);
        }
        IdentityMutation::CheckScriptHandle(_) => {
            let valid = validate_node_handle(core, state.id).is_ok();
            prop_assert_eq!(valid, state.live);
        }
    }

    assert_identity_model(core, model)
}

proptest! {
    #[test]
    fn identity_state_machine_matches_core(actions in identity_mutation_strategy()) {
        let mut core = Core::new();
        let mut model = Vec::with_capacity(IDENTITY_NODE_COUNT);
        for _ in 0..IDENTITY_NODE_COUNT {
            model.push(IdentityNodeModel {
                id: core.create_detached(FocusableWidget)?,
                live: true,
                attached: false,
                focusable: true,
                hidden: false,
            });
        }
        assert_identity_model(&core, &model)?;

        for (index, action) in actions.iter().enumerate() {
            trace_result(
                apply_identity_mutation(&mut core, &mut model, action),
                &actions,
                index,
            )?;
        }
    }
}

#[test]
fn validate_invariants_accepts_laid_out_tree() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    let child = core.create_detached(simple_widget())?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.update_layout(Size::new(10, 10))?;
    core.validate_invariants()
}

#[test]
fn focus_transition_reports_changes_and_rejects_invalid_targets() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget)?;
    assert!(matches!(core.set_focus(child), Err(Error::NodeDetached(id)) if id == child));

    core.attach(core.root, child)?;
    assert_eq!(core.set_focus(child)?, ChangeOutcome::Changed);
    assert_eq!(core.set_focus(child)?, ChangeOutcome::Unchanged);
    assert_eq!(core.clear_focus()?, ChangeOutcome::Changed);
    assert_eq!(core.clear_focus()?, ChangeOutcome::Unchanged);

    core.remove_subtree(child)?;
    assert!(matches!(core.set_focus(child), Err(Error::NodeNotFound(id)) if id == child));
    core.validate_invariants()
}

#[test]
fn focus_path_queries_tolerate_a_stale_internal_id() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget)?;
    core.attach(core.root, child)?;
    core.remove_subtree(child)?;

    core.focus = Some(child);
    assert!(!core.is_on_focus_path(core.root));
    assert_eq!(core.focus_path(core.root), Path::empty());
    core.ensure_focus_valid(None)?;
    assert_eq!(core.focus_id(), None);
    Ok(())
}

#[test]
fn mouse_capture_transition_reports_changes_and_rejects_invalid_targets() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    assert!(matches!(core.capture_mouse(child), Err(Error::NodeDetached(id)) if id == child));

    core.attach(core.root, child)?;
    assert_eq!(core.capture_mouse(child)?, ChangeOutcome::Changed);
    assert_eq!(core.capture_mouse(child)?, ChangeOutcome::Unchanged);
    assert_eq!(core.release_mouse(core.root)?, ChangeOutcome::Unchanged);
    assert_eq!(core.release_mouse(child)?, ChangeOutcome::Changed);
    assert_eq!(core.release_mouse(child)?, ChangeOutcome::Unchanged);

    core.remove_subtree(child)?;
    assert!(matches!(core.capture_mouse(child), Err(Error::NodeNotFound(id)) if id == child));
    core.validate_invariants()
}

#[test]
fn visibility_transition_reports_changes_and_missing_nodes() -> Result<()> {
    let mut core = Core::new();
    let node = core.create_detached(simple_widget())?;
    assert_eq!(core.set_hidden(node, true)?, ChangeOutcome::Changed);
    assert_eq!(core.set_hidden(node, true)?, ChangeOutcome::Unchanged);
    assert_eq!(core.set_hidden(node, false)?, ChangeOutcome::Changed);

    core.remove_subtree(node)?;
    assert!(matches!(
        core.set_hidden(node, true),
        Err(Error::NodeNotFound(id)) if id == node
    ));
    Ok(())
}

#[test]
fn validate_invariants_rejects_missing_child_link() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    let child = core.create_detached(simple_widget())?;
    core.set_children(parent, vec![child])?;
    core.nodes.remove(child);

    let error = core
        .validate_invariants()
        .expect_err("missing child should fail validation");
    assert!(matches!(error, Error::Invariant(_)));
    Ok(())
}

#[test]
fn validate_invariants_rejects_initialized_attached_unmounted_node() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.mounted = false;
        node.initialized = true;
    }

    let error = core
        .validate_invariants()
        .expect_err("initialized attached node must be mounted");
    assert!(matches!(error, Error::Invariant(_)));
    Ok(())
}

#[test]
fn read_only_widget_access_allows_nested_reads() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;

    let nested = core.with_widget_read(
        child,
        WidgetOperation::access("outer read"),
        |_widget, core| {
            core.with_widget_read(
                child,
                WidgetOperation::access("inner read"),
                |_widget, _core| true,
            )
        },
    )??;

    assert!(nested);
    Ok(())
}

#[test]
fn widget_read_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();

    let error = core
        .with_widget_mut(child, |_widget, core| {
            core.with_widget_read(
                child,
                WidgetOperation::access("test read"),
                |_widget, _core| (),
            )
        })?
        .expect_err("nested read should fail while the widget is extracted");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Access,
            ..
        }
    ));
    assert_error_context(&error, "test read", child, &path);
    Ok(())
}

#[test]
fn widget_slot_restores_after_nested_access_error() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;

    core.with_widget_mut(child, |_widget, core| {
        let nested = core.with_widget_mut(child, |_widget, _core| ());
        let error = nested.expect_err("nested mutation should fail");
        assert!(matches!(
            error,
            Error::NodeOperation {
                kind: NodeOperationKind::Access,
                ..
            }
        ));
        assert_error_context(&error, "mutation callback", child, "<detached>");
    })?;
    core.with_widget_mut(child, |_widget, _core| ())?;

    Ok(())
}

#[test]
fn layout_refresh_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();
    core.nodes[child].layout_dirty = true;

    let error = core
        .with_widget_mut(child, |_widget, core| refresh_layouts(core))?
        .expect_err("layout refresh should fail while the widget is extracted");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Layout,
            ..
        }
    ));
    assert_error_context(&error, "layout refresh", child, &path);
    Ok(())
}

#[test]
fn widget_slot_restores_after_callback_error() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;

    let result = core.with_widget_mut(child, |_widget, _core| -> Result<()> {
        Err(Error::Invalid("callback failed".into()))
    })?;
    assert!(matches!(result, Err(Error::Invalid(_))));
    core.with_widget_mut(child, |_widget, _core| ())?;

    Ok(())
}

#[test]
fn widget_slot_restores_after_callback_panic() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ignored = core.with_widget_mut(child, |_widget, _core| {
            panic!("callback panic");
        });
    }));

    assert!(result.is_err());
    core.with_widget_mut(child, |_widget, _core| ())?;
    Ok(())
}

#[test]
fn callback_cannot_remove_current_node() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    let path = core.node_path(core.root, nodes.current).to_string();

    let error = with_callback_context(&mut core, nodes.current, |ctx| {
        ctx.remove_subtree(nodes.current)
    })?
    .expect_err("callback should not remove the current node");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Access,
            ..
        }
    ));
    assert_error_context(&error, "remove subtree", nodes.current, &path);
    assert!(core.nodes.contains_key(nodes.current));
    core.validate_invariants()
}

#[test]
fn callback_cannot_replace_current_node() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    let path = core.node_path(core.root, nodes.current).to_string();

    let error = with_callback_core(&mut core, nodes.current, |core| {
        core.replace_subtree(nodes.current, simple_widget())
    })?
    .expect_err("callback should not replace the current node");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Access,
            ..
        }
    ));
    assert_error_context(&error, "replace subtree", nodes.current, &path);
    assert_eq!(
        core.nodes[nodes.current].widget_type,
        TypeId::of::<TestWidget>()
    );
    core.validate_invariants()
}

#[test]
fn callback_cannot_remove_parent_containing_current_node() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    let path = core.node_path(core.root, nodes.current).to_string();

    let error = with_callback_context(&mut core, nodes.current, |ctx| {
        ctx.remove_subtree(nodes.parent)
    })?
    .expect_err("callback should not remove the current node's parent");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Access,
            ..
        }
    ));
    assert_error_context(&error, "remove subtree", nodes.current, &path);
    assert!(core.nodes.contains_key(nodes.parent));
    assert!(core.nodes.contains_key(nodes.current));
    core.validate_invariants()
}

#[test]
fn callback_cannot_replace_parent_containing_current_node() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    let path = core.node_path(core.root, nodes.current).to_string();

    let error = with_callback_core(&mut core, nodes.current, |core| {
        core.replace_subtree(nodes.parent, simple_widget())
    })?
    .expect_err("callback should not replace the current node's parent");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Access,
            ..
        }
    ));
    assert_error_context(&error, "replace subtree", nodes.current, &path);
    assert!(core.nodes.contains_key(nodes.parent));
    assert!(core.nodes.contains_key(nodes.current));
    core.validate_invariants()
}

#[test]
fn callback_removes_sibling_immediately() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;

    with_callback_context(&mut core, nodes.current, |ctx| {
        ctx.remove_subtree(nodes.sibling)
    })??;

    assert!(!core.nodes.contains_key(nodes.sibling));
    assert!(!core.nodes[nodes.parent].children.contains(&nodes.sibling));
    core.validate_invariants()
}

#[test]
fn callback_replaces_sibling_immediately() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;

    with_callback_core(&mut core, nodes.current, |core| {
        core.replace_subtree(nodes.sibling, FocusableWidget)
    })??;

    assert!(core.nodes.contains_key(nodes.sibling));
    assert_eq!(
        core.nodes[nodes.sibling].widget_type,
        TypeId::of::<FocusableWidget>()
    );
    core.validate_invariants()
}

#[test]
fn callback_removing_focused_node_recovers_focus() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    core.set_focus(nodes.focused)?;

    with_callback_context(&mut core, nodes.current, |ctx| {
        ctx.remove_subtree(nodes.focused)
    })??;

    assert!(!core.nodes.contains_key(nodes.focused));
    assert_eq!(core.focus, Some(nodes.focus_fallback));
    core.validate_invariants()
}

#[test]
fn callback_removing_mouse_capture_node_clears_capture() -> Result<()> {
    let (mut core, nodes) = callback_mutation_tree()?;
    core.capture_mouse(nodes.mouse_capture)?;

    with_callback_context(&mut core, nodes.current, |ctx| {
        ctx.remove_subtree(nodes.mouse_capture)
    })??;

    assert!(!core.nodes.contains_key(nodes.mouse_capture));
    assert!(core.mouse_capture.is_none());
    core.validate_invariants()
}

#[test]
fn set_children_detaches_from_previous_parent() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent_a = core.create_detached(parent_widget)?;
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent_b = core.create_detached(parent_widget)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.create_detached(child_widget)?;

    core.set_children(parent_a, vec![child])?;
    core.set_children(parent_b, vec![child])?;

    assert!(core.nodes[parent_a].children.is_empty());
    assert_eq!(core.nodes[parent_b].children, vec![child]);
    assert_eq!(core.nodes[child].parent, Some(parent_b));
    Ok(())
}

#[test]
fn set_children_rejects_cycles() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.create_detached(child_widget)?;
    core.set_children(parent, vec![child])?;

    let err = core.set_children(child, vec![parent]).unwrap_err();
    assert!(matches!(err, Error::WouldCreateCycle { .. }));
    Ok(())
}

#[test]
fn set_children_rejects_duplicates() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.create_detached(child_widget)?;

    let err = core
        .set_children(parent, vec![child, child])
        .expect_err("expected duplicate child error");
    assert!(matches!(
        err,
        Error::DuplicateChild {
            parent: err_parent,
            child: err_child,
        } if err_parent == parent && err_child == child
    ));
    Ok(())
}

#[test]
fn attach_rejects_cycles() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.create_detached(child_widget)?;

    core.attach(parent, child)?;
    let err = core.attach(child, parent).unwrap_err();
    assert!(matches!(err, Error::WouldCreateCycle { .. }));
    Ok(())
}

#[test]
fn remove_subtree_recovers_focus_to_next() -> Result<()> {
    let mut core = Core::new();
    let first = core.create_detached(FocusableWidget)?;
    let second = core.create_detached(FocusableWidget)?;
    core.set_children(core.root, vec![first, second])?;
    core.with_layout_of(core.root, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(first, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(second, |layout| {
        *layout = Layout::fill();
    })?;
    core.update_layout(Size::new(10, 10))?;

    core.set_focus(first)?;
    core.remove_subtree(first)?;

    assert_eq!(core.focus, Some(second));
    Ok(())
}

#[test]
fn remove_subtree_recovers_focus_to_prev() -> Result<()> {
    let mut core = Core::new();
    let first = core.create_detached(FocusableWidget)?;
    let second = core.create_detached(FocusableWidget)?;
    core.set_children(core.root, vec![first, second])?;
    core.with_layout_of(core.root, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(first, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(second, |layout| {
        *layout = Layout::fill();
    })?;
    core.update_layout(Size::new(10, 10))?;

    core.set_focus(second)?;
    core.remove_subtree(second)?;

    assert_eq!(core.focus, Some(first));
    Ok(())
}

#[test]
fn detach_clears_mouse_capture() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget)?;
    core.attach(core.root, child)?;
    core.capture_mouse(child)?;

    core.detach(child)?;

    assert!(core.mouse_capture.is_none());
    assert!(core.nodes.get(child).is_some());
    assert!(core.nodes[child].parent.is_none());
    Ok(())
}

#[test]
fn remove_subtree_clears_mouse_capture() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget)?;
    core.attach(core.root, child)?;
    core.capture_mouse(child)?;

    core.remove_subtree(child)?;

    assert!(core.mouse_capture.is_none());
    assert!(core.nodes.get(child).is_none());
    Ok(())
}

#[test]
fn keyed_children_require_unique_keys() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    core.attach(core.root, parent)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_child_to_keyed_boxed(parent, "slot", Box::new(child_widget))?;
    let node_count = core.nodes.len();

    let (other_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let err = core
        .add_child_to_keyed_boxed(parent, "slot", Box::new(other_widget))
        .unwrap_err();

    assert!(matches!(err, Error::DuplicateChildKey(_)));
    assert_eq!(core.nodes.len(), node_count);
    assert_eq!(core.child_keyed(parent, "slot"), Some(child));
    Ok(())
}

#[test]
fn detach_clears_keyed_mapping() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    core.attach(core.root, parent)?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_child_to_keyed_boxed(parent, "slot", Box::new(child_widget))?;

    core.detach(child)?;

    assert!(core.child_keyed(parent, "slot").is_none());
    assert!(core.nodes[child].parent.is_none());
    Ok(())
}

#[test]
fn add_child_rolls_back_on_mount_failure() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget)?;
    core.attach(core.root, parent)?;
    let node_count = core.nodes.len();

    let err = core
        .add_child_to_boxed(parent, Box::new(MountFailWidget))
        .unwrap_err();

    assert!(matches!(err, Error::Invalid(_)));
    assert_eq!(core.nodes.len(), node_count);
    assert!(core.nodes[parent].children.is_empty());
    Ok(())
}

#[test]
fn set_children_fault_restores_core_state_and_unwinds_completed_mounts() -> Result<()> {
    let mut core = Core::new();
    core.set_focus(core.root)?;
    core.capture_mouse(core.root)?;
    core.pending_help_request = Some((core.root, Some(core.root)));
    core.pending_diagnostic_dump = Some(core.root);
    let log = Arc::new(Mutex::new(Vec::new()));
    let mounted = core.create_detached(
        FaultWidget::new("mounted", Arc::clone(&log))
            .with_unmount(UnmountAction::TryStructuralEdit),
    )?;
    let failing = core.create_detached(
        FaultWidget::new("failing", Arc::clone(&log))
            .with_mount(MountAction::FailAfterCoreMutations),
    )?;

    let error = assert_structural_rollback(&mut core, |core| {
        core.set_children(core.root, vec![mounted, failing])
    });

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            HookEvent::Mount("mounted"),
            HookEvent::Mount("failing"),
            HookEvent::Unmount("mounted"),
            HookEvent::RollbackEditRejected(true),
        ]
    );
    Ok(())
}

#[test]
fn nested_mount_failure_joins_outer_tree_edit() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let parent = core.create_detached(
        FaultWidget::new("parent", Arc::clone(&log)).with_mount(MountAction::FailFromNestedEdit),
    )?;

    let error = assert_structural_rollback(&mut core, |core| core.attach(core.root, parent));

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(*log.lock().unwrap(), vec![HookEvent::Mount("parent")]);
    Ok(())
}

#[test]
fn handled_nested_failure_restores_its_savepoint() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let parent = core.create_detached(
        FaultWidget::new("parent", Arc::clone(&log)).with_mount(MountAction::HandleNestedFailure),
    )?;

    core.attach(core.root, parent)?;

    assert_eq!(*log.lock().unwrap(), vec![HookEvent::Mount("parent")]);
    assert!(core.nodes[parent].children.is_empty());
    assert_eq!(core.nodes.len(), 2);
    core.validate_invariants()
}

#[test]
fn pre_remove_nested_edit_rolls_back_with_outer_failure() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let sibling = core.create_detached(FaultWidget::new("sibling", Arc::clone(&log)))?;
    let target = core.create_detached(
        FaultWidget::new("target", Arc::clone(&log))
            .with_pre_remove(PreRemoveAction::RemoveThenFail(sibling)),
    )?;
    core.set_children(core.root, vec![target, sibling])?;
    log.lock().unwrap().clear();

    let error = assert_structural_rollback(&mut core, |core| core.remove_subtree(target));

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            HookEvent::PreRemove("target"),
            HookEvent::PreRemove("sibling"),
            HookEvent::Unmount("sibling"),
        ]
    );
    Ok(())
}

#[test]
fn replacement_mount_failure_restores_old_widget_and_children() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let target = core.create_detached(FaultWidget::new("old", Arc::clone(&log)))?;
    let child = core.create_detached(simple_widget())?;
    core.set_children(target, vec![child])?;
    core.attach(core.root, target)?;
    log.lock().unwrap().clear();

    let error = assert_structural_rollback(&mut core, |core| {
        core.replace_subtree(
            target,
            FaultWidget::new("new", Arc::clone(&log))
                .with_mount(MountAction::FailAfterCoreMutations),
        )
    });

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            HookEvent::PreRemove("old"),
            HookEvent::Unmount("old"),
            HookEvent::Mount("new"),
        ]
    );
    Ok(())
}

#[test]
fn replace_subtree_runs_complete_lifecycle_in_tree_order() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let target = core.create_detached(FaultWidget::new("old", Arc::clone(&log)))?;
    let child = core.create_detached(FaultWidget::new("child", Arc::clone(&log)))?;
    core.set_children(target, vec![child])?;
    core.attach(core.root, target)?;
    log.lock().unwrap().clear();

    core.replace_subtree(target, FaultWidget::new("new", Arc::clone(&log)))?;

    assert!(!core.nodes.contains_key(child));
    assert!(core.nodes[target].children.is_empty());
    assert_eq!(
        *log.lock().unwrap(),
        vec![
            HookEvent::PreRemove("old"),
            HookEvent::PreRemove("child"),
            HookEvent::Unmount("child"),
            HookEvent::Unmount("old"),
            HookEvent::Mount("new"),
        ]
    );
    core.validate_invariants()
}

#[test]
fn keyed_child_add_rolls_back_key_and_node_on_mount_failure() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    core.attach(core.root, parent)?;

    let error = assert_structural_rollback(&mut core, |core| {
        core.add_child_to_keyed_boxed(parent, "fault", Box::new(MountFailWidget))?;
        Ok(())
    });

    assert!(matches!(error, Error::Invalid(_)));
    assert!(core.child_keyed(parent, "fault").is_none());
    Ok(())
}

#[test]
fn pre_remove_veto_leaves_subtree_mounted() -> Result<()> {
    let mut core = Core::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    let target = core.create_detached(
        FaultWidget::new("target", Arc::clone(&log)).with_pre_remove(PreRemoveAction::Fail),
    )?;
    core.attach(core.root, target)?;
    log.lock().unwrap().clear();

    let error = assert_structural_rollback(&mut core, |core| core.remove_subtree(target));

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(*log.lock().unwrap(), vec![HookEvent::PreRemove("target")]);
    Ok(())
}

#[test]
fn keyed_reconcile_update_failure_preserves_core_and_helper_state() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    core.attach(core.root, parent)?;
    let mut keyed = KeyedChildren::<&'static str, ReconcileWidget>::new();
    let before = StructuralSnapshot::capture(&core);

    let error = {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed
            .reconcile(
                &mut ctx,
                ["a", "b"],
                |_key| Ok(ReconcileWidget::succeeds()),
                |key, id, ctx| {
                    if *key == "a" {
                        ctx.set_hidden_of(id.into(), true)?;
                        Ok(())
                    } else {
                        Err(Error::Invalid("reconcile update failure".into()))
                    }
                },
                RemovePolicy::RemoveSubtree,
            )
            .expect_err("update failure should abort reconcile")
    };

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(StructuralSnapshot::capture(&core), before);
    assert!(keyed.is_empty());
    core.validate_invariants()
}

#[test]
fn keyed_reconcile_mount_failure_preserves_core_and_helper_state() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    core.attach(core.root, parent)?;
    let mut keyed = KeyedChildren::<&'static str, ReconcileWidget>::new();
    let before = StructuralSnapshot::capture(&core);

    let error = {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed
            .reconcile(
                &mut ctx,
                ["a", "b"],
                |key| {
                    Ok(if *key == "b" {
                        ReconcileWidget::fails_mount()
                    } else {
                        ReconcileWidget::succeeds()
                    })
                },
                |_key, _id, _ctx| Ok(()),
                RemovePolicy::RemoveSubtree,
            )
            .expect_err("mount failure should abort reconcile")
    };

    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(StructuralSnapshot::capture(&core), before);
    assert!(keyed.is_empty());
    core.validate_invariants()
}

#[test]
fn keyed_reconcile_prunes_removed_hidden_nodes() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    core.attach(core.root, parent)?;
    let mut keyed = KeyedChildren::<&'static str, ReconcileWidget>::new();

    {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed.reconcile(
            &mut ctx,
            ["a", "b"],
            |_key| Ok(ReconcileWidget::succeeds()),
            |_key, _id, _ctx| Ok(()),
            RemovePolicy::Hide,
        )?;
    }
    let removed = keyed.id_for(&"b").expect("b should exist");
    {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed.reconcile(
            &mut ctx,
            ["a"],
            |_key| Ok(ReconcileWidget::succeeds()),
            |_key, _id, _ctx| Ok(()),
            RemovePolicy::Hide,
        )?;
    }
    assert!(core.nodes[removed.into()].parent.is_none());
    assert!(core.nodes[removed.into()].hidden);
    core.remove_subtree(removed)?;

    {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed.reconcile(
            &mut ctx,
            ["a", "b"],
            |_key| Ok(ReconcileWidget::succeeds()),
            |_key, _id, _ctx| Ok(()),
            RemovePolicy::Hide,
        )?;
    }

    let replacement = keyed.id_for(&"b").expect("b should be recreated");
    assert_ne!(replacement, removed);
    assert_eq!(keyed.keys(), &["a", "b"]);
    assert_eq!(
        core.nodes[parent].children,
        vec![keyed.id_for(&"a").unwrap().into(), replacement.into()]
    );
    core.validate_invariants()
}

#[test]
fn keyed_reconcile_defers_removal_until_updates_succeed() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget())?;
    core.attach(core.root, parent)?;
    let mut keyed = KeyedChildren::<&'static str, ReconcileWidget>::new();
    {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed.reconcile(
            &mut ctx,
            ["a", "b"],
            |_key| Ok(ReconcileWidget::succeeds()),
            |_key, _id, _ctx| Ok(()),
            RemovePolicy::RemoveSubtree,
        )?;
    }
    let before = StructuralSnapshot::capture(&core);
    let a = keyed.id_for(&"a").expect("a should exist");

    {
        let mut ctx = CoreContext::new(&mut core, parent);
        keyed
            .reconcile(
                &mut ctx,
                ["b", "c"],
                |_key| Ok(ReconcileWidget::succeeds()),
                |key, _id, _ctx| {
                    if *key == "c" {
                        Err(Error::Invalid("late update failure".into()))
                    } else {
                        Ok(())
                    }
                },
                RemovePolicy::RemoveSubtree,
            )
            .expect_err("late update failure should abort reconcile");
    }

    assert_eq!(StructuralSnapshot::capture(&core), before);
    assert_eq!(keyed.keys(), &["a", "b"]);
    assert_eq!(keyed.id_for(&"a"), Some(a));
    assert!(core.nodes.contains_key(a.into()));
    core.validate_invariants()
}
