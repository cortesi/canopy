use std::{
    any::TypeId,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

use proptest::{prelude::*, test_runner::TestCaseResult};
use rand::{RngExt, SeedableRng, rngs::StdRng};

use super::{
    layout_driver::{
        LayoutPass, align_offset, allocate_flex_shares, clamp_outer, clamp_scroll,
        constraint_for_axis, refresh_layouts,
    },
    *,
};
use crate::{
    Context, KeyedChildren, RemovePolicy,
    core::{
        context::{CoreContext, CoreViewContext},
        script::validate_node_handle,
    },
    error::{Error, Result},
    geom::{Point, Size},
    layout::{
        Align, CanvasContext, Constraint, Direction, Direction as LayoutDirection, Display, Edges,
        Layout, MeasureConstraints, Measurement, Sizing,
    },
    path::Path,
    widget::Widget,
};

type MeasureFn = dyn Fn(MeasureConstraints) -> Measurement + Send + Sync;
type CanvasFn = dyn Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync;

struct TestWidget {
    measure_fn: Arc<MeasureFn>,
    canvas_fn: Arc<CanvasFn>,
}

impl TestWidget {
    fn new<F>(measure_fn: F) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
    where
        F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
    {
        Self::with_canvas(measure_fn, |view, _ctx| view)
    }

    fn with_canvas<F, C>(measure_fn: F, canvas_fn: C) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
    where
        F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
        C: Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync + 'static,
    {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let measure_fn = Arc::new(move |c: MeasureConstraints| {
            calls_clone.lock().unwrap().push(c);
            measure_fn(c)
        });
        let canvas_fn = Arc::new(canvas_fn);
        (
            Self {
                measure_fn,
                canvas_fn,
            },
            calls,
        )
    }
}

impl Widget for TestWidget {
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        (self.measure_fn)(c)
    }

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
        (self.canvas_fn)(view, ctx)
    }
}

struct FocusableWidget;

impl Widget for FocusableWidget {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }
}

struct LayoutWidget(Layout);

impl Widget for LayoutWidget {
    fn layout(&self) -> Layout {
        self.0
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
        context.typed_id::<TestWidget>(original)?
    };
    assert_eq!(NodeId::from(typed), original);

    let wrong_type = {
        let context = CoreContext::new(&mut core, original);
        let context: &dyn Context = &context;
        context.typed_id::<FocusableWidget>(original)
    };
    assert!(matches!(wrong_type, Err(Error::NodeTypeMismatch { .. })));

    core.remove_subtree(original)?;
    let replacement = core.create_detached(simple_widget())?;
    assert_ne!(replacement, original);
    let stale = {
        let context = CoreContext::new(&mut core, replacement);
        let context: &dyn Context = &context;
        context.typed_id::<TestWidget>(original)
    };
    assert!(matches!(stale, Err(Error::NodeNotFound(id)) if id == original));
    Ok(())
}

fn assert_error_context(error: &Error, operation: &str, node_id: NodeId, path: &str) {
    let message = error.to_string();
    assert!(
        message.contains(operation),
        "expected {message:?} to contain operation {operation:?}"
    );
    assert!(
        message.contains(&format!("{node_id:?}")),
        "expected {message:?} to contain node ID {node_id:?}"
    );
    assert!(
        message.contains(path),
        "expected {message:?} to contain path {path:?}"
    );
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

        for mutation in mutations {
            apply_tree_mutation(&mut core, &mut nodes, &mutation)?;
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
            let context: &dyn ReadContext = &context;
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

        for action in actions {
            apply_identity_mutation(&mut core, &mut model, &action)?;
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
    assert!(core.focus_path_ids().is_empty());
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

    assert!(matches!(error, Error::WidgetAccess(_)));
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
        assert!(matches!(error, Error::WidgetAccess(_)));
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

    assert!(matches!(error, Error::Layout(_)));
    assert_error_context(&error, "layout refresh", child, &path);
    Ok(())
}

#[test]
fn measure_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();
    let constraints = MeasureConstraints {
        width: Constraint::AtMost(1),
        height: Constraint::AtMost(1),
    };

    let error = core
        .with_widget_mut(child, |_widget, core| {
            let mut pass = LayoutPass::new(core);
            pass.measure_cached(child, constraints)
        })?
        .expect_err("measure should fail while the widget is extracted");

    assert!(matches!(error, Error::Layout(_)));
    assert_error_context(&error, "measure", child, &path);
    Ok(())
}

#[test]
fn canvas_errors_include_operation_node_and_path() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(simple_widget())?;
    attach_root_child(&mut core, child)?;
    let path = core.node_path(core.root, child).to_string();

    let error = core
        .with_widget_mut(child, |_widget, core| {
            let pass = LayoutPass::new(core);
            pass.compute_canvas(child, Size::new(1, 1))
        })?
        .expect_err("canvas should fail while the widget is extracted");

    assert!(matches!(error, Error::Layout(_)));
    assert_error_context(&error, "canvas", child, &path);
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

    assert!(matches!(error, Error::WidgetAccess(_)));
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

    assert!(matches!(error, Error::WidgetAccess(_)));
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

    assert!(matches!(error, Error::WidgetAccess(_)));
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

    assert!(matches!(error, Error::WidgetAccess(_)));
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
fn clamp_outer_no_bounds() {
    let layout = Layout::column();
    let size = Size::new(5, 7);
    assert_eq!(clamp_outer(size, layout), size);
}

#[test]
fn clamp_outer_min_only() {
    let mut layout = Layout::column();
    layout.min_width = Some(10);
    layout.min_height = Some(2);
    assert_eq!(clamp_outer(Size::new(5, 1), layout), Size::new(10, 2));
}

#[test]
fn clamp_outer_max_only() {
    let mut layout = Layout::column();
    layout.max_width = Some(3);
    layout.max_height = Some(4);
    assert_eq!(clamp_outer(Size::new(5, 7), layout), Size::new(3, 4));
}

#[test]
fn constraint_for_axis_flex_is_exact() {
    let c = constraint_for_axis(Sizing::Flex(1), 10, None, None, 0, false);
    assert_eq!(c, Constraint::Exact(10));
}

#[test]
fn constraint_for_axis_min_equals_max_is_exact() {
    let c = constraint_for_axis(Sizing::Measure, 10, Some(6), Some(6), 2, false);
    assert_eq!(c, Constraint::Exact(4));
}

#[test]
fn constraint_for_axis_max_caps_available() {
    let c = constraint_for_axis(Sizing::Measure, 10, None, Some(6), 2, false);
    assert_eq!(c, Constraint::AtMost(4));
}

#[test]
fn leaf_measure_adds_padding() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().padding(Edges::all(1));
    })?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[child];
    assert_eq!(node.rect.w, 7);
    assert_eq!(node.rect.h, 7);
    assert_eq!(node.content_size, Size::new(5, 5));
    Ok(())
}

#[test]
fn leaf_padding_consumes_all() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill().padding(Edges::all(1));
    })?;
    core.update_layout(Size::new(1, 1))?;
    let node = &core.nodes[child];
    assert_eq!(node.content_size, Size::new(0, 0));
    Ok(())
}

#[test]
fn flex_axis_constraints_are_exact() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().flex_horizontal(1);
    })?;
    core.update_layout(Size::new(10, 5))?;
    let calls = calls.lock().unwrap();
    assert!(!calls.is_empty());
    assert_eq!(calls[0].width, Constraint::Exact(10));
    Ok(())
}

#[test]
fn remeasure_when_min_width_expands_measured() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            _ => 4,
        };
        let height = if width >= 10 { 2 } else { 4 };
        Measurement::Fixed(Size::new(width, height))
    });
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().min_width(10);
    })?;
    core.update_layout(Size::new(20, 20))?;
    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(10)));
    let node = &core.nodes[child];
    assert_eq!(node.content_size.h, 2);
    Ok(())
}

#[test]
fn remeasure_when_min_width_expands_flex() -> Result<()> {
    let mut core = Core::new();
    let (widget, calls) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            _ => 0,
        };
        Measurement::Fixed(Size::new(width, width.max(1)))
    });
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column()
            .flex_horizontal(1)
            .padding(Edges::all(1))
            .min_width(30);
    })?;
    core.update_layout(Size::new(10, 10))?;
    let calls = calls.lock().unwrap();
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(8)));
    assert!(calls.iter().any(|c| c.width == Constraint::Exact(28)));
    Ok(())
}

#[test]
fn wrap_no_children() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::column().padding(Edges::all(1));
    })?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(0, 0));
    assert_eq!(node.rect.w, 2);
    assert_eq!(node.rect.h, 2);
    Ok(())
}

#[test]
fn wrap_sum_main_max_cross() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 3)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    let child3 = core.add_boxed(Box::new(c3))?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::column().gap(1);
    })?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(4, 8));
    Ok(())
}

#[test]
fn wrap_includes_child_padding() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 1)));
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::column();
    })?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().padding(Edges::all(1));
    })?;
    core.update_layout(Size::new(50, 50))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size, Size::new(5, 3));
    Ok(())
}

#[test]
fn wrap_flex_child_treated_as_measure_when_parent_not_exact() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 4)));
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::column();
    })?;
    core.with_layout_of(child, |layout| {
        layout.height = Sizing::Flex(1);
    })?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size.h, 4);
    Ok(())
}

#[test]
fn wrap_flex_child_behaves_as_flex_when_parent_exact() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child1_widget, calls1) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            Constraint::AtMost(n) => n,
            Constraint::Unbounded => 0,
        };
        Measurement::Fixed(Size::new(width, width))
    });
    let (child2_widget, calls2) = TestWidget::new(|c| {
        let width = match c.width {
            Constraint::Exact(n) => n,
            Constraint::AtMost(n) => n,
            Constraint::Unbounded => 0,
        };
        Measurement::Fixed(Size::new(width, width))
    });
    let child1 = core.add_boxed(Box::new(child1_widget))?;
    let child2 = core.add_boxed(Box::new(child2_widget))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::row().flex_horizontal(1);
    })?;
    core.with_layout_of(child1, |layout| {
        layout.width = Sizing::Flex(1);
    })?;
    core.with_layout_of(child2, |layout| {
        layout.width = Sizing::Flex(1);
    })?;
    core.update_layout(Size::new(10, 10))?;
    let calls1 = calls1.lock().unwrap();
    let calls2 = calls2.lock().unwrap();
    assert!(calls1.iter().any(|c| c.width == Constraint::Exact(5)));
    assert!(calls2.iter().any(|c| c.width == Constraint::Exact(5)));
    let parent_node = &core.nodes[parent];
    assert_eq!(parent_node.content_size.h, 5);
    Ok(())
}

#[test]
fn wrap_gap_counts_only_visible_children() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    let child3 = core.add_boxed(Box::new(c3))?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::column().gap(2);
    })?;
    core.with_layout_of(child2, |layout| {
        *layout = Layout::column().none();
    })?;
    core.update_layout(Size::new(20, 20))?;
    let node = &core.nodes[parent];
    assert_eq!(node.content_size.h, 4);
    Ok(())
}

#[test]
fn flex_shares_sum_equals_remaining() {
    let shares = allocate_flex_shares(17, &[1, 2, 3, 4]);
    let sum: u32 = shares.iter().sum();
    assert_eq!(sum, 17);
}

#[test]
fn flex_shares_proportional_sanity() {
    let shares = allocate_flex_shares(5, &[3, 7]);
    assert_eq!(shares, vec![2, 3]);
}

#[test]
fn flex_shares_stable_tie_break() {
    let shares = allocate_flex_shares(2, &[1, 1, 1]);
    assert_eq!(shares, vec![1, 1, 0]);
}

fn boundary_layout_u32() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(0),
        Just(1),
        Just(2),
        Just(u32::MAX - 1),
        Just(u32::MAX),
        any::<u32>(),
    ]
}

proptest! {
    #[test]
    fn flex_shares_preserve_remaining_space(remaining in 0u32..1000, weights in prop::collection::vec(1u32..20, 0..12)) {
        let shares = allocate_flex_shares(remaining, &weights);
        prop_assert_eq!(shares.len(), weights.len());
        let expected = if weights.is_empty() { 0 } else { remaining };
        prop_assert_eq!(shares.iter().sum::<u32>(), expected);
        prop_assert!(shares.iter().all(|share| *share <= remaining));
    }

    #[test]
    fn clamp_scroll_stays_inside_canvas(
        scroll_x in 0u32..200,
        scroll_y in 0u32..200,
        view_w in 0u32..100,
        view_h in 0u32..100,
        canvas_w in 0u32..150,
        canvas_h in 0u32..150,
    ) {
        let mut scroll = Point { x: scroll_x, y: scroll_y };
        let view = Size::new(view_w, view_h);
        let canvas = Size::new(canvas_w, canvas_h);

        clamp_scroll(&mut scroll, view, canvas);

        let max_x = if view.w == 0 { 0 } else { canvas.w.saturating_sub(view.w) };
        let max_y = if view.h == 0 { 0 } else { canvas.h.saturating_sub(view.h) };
        prop_assert!(scroll.x <= max_x);
        prop_assert!(scroll.y <= max_y);
    }

    #[test]
    fn hidden_display_and_padding_layout_properties(
        hidden in any::<bool>(),
        display_none in any::<bool>(),
        padding in 0u32..8,
        screen_w in 0u32..40,
        screen_h in 0u32..40,
    ) {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget))?;
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let child = core.add_boxed(Box::new(child_widget))?;

        prop_assert!(core.set_children(parent, vec![child]).is_ok());
        prop_assert!(attach_root_child(&mut core, parent).is_ok());
        let parent_layout_set = core.with_layout_of(parent, |layout| {
            *layout = Layout::fill().padding(Edges::all(padding));
        }).is_ok();
        prop_assert!(parent_layout_set);
        let child_layout_set = core.with_layout_of(child, |layout| {
            *layout = if display_none {
                Layout::fill().none()
            } else {
                Layout::fill()
            };
        }).is_ok();
        prop_assert!(child_layout_set);
        core.set_hidden(child, hidden)?;

        prop_assert!(core.update_layout(Size::new(screen_w, screen_h)).is_ok());

        let parent_node = &core.nodes[parent];
        let expected_content = Size::new(
            parent_node.rect.w.saturating_sub(parent_node.layout.padding.horizontal()),
            parent_node.rect.h.saturating_sub(parent_node.layout.padding.vertical()),
        );
        prop_assert_eq!(parent_node.content_size, expected_content);

        let child_node = &core.nodes[child];
        if hidden || display_none {
            prop_assert_eq!(child_node.rect.w, 0);
            prop_assert_eq!(child_node.rect.h, 0);
            prop_assert_eq!(child_node.canvas, Size::ZERO);
            prop_assert!(child_node.view.is_zero());
        } else {
            prop_assert!(child_node.rect.w <= parent_node.content_size.w);
            prop_assert!(child_node.rect.h <= parent_node.content_size.h);
        }
    }

    #[test]
    fn sequential_layout_boundaries_preserve_alignment_and_order(
        is_row in any::<bool>(),
        main_align in 0u8..3,
        cross_align in 0u8..3,
        padding in 0u32..4,
        gap in boundary_layout_u32(),
        screen_w in boundary_layout_u32(),
        screen_h in boundary_layout_u32(),
        hide_last in any::<bool>(),
        remove_last in any::<bool>(),
        overflow in any::<bool>(),
    ) {
        let direction = if is_row { Direction::Row } else { Direction::Column };
        let align = |value| match value {
            0 => Align::Start,
            1 => Align::Center,
            _ => Align::End,
        };
        let (horizontal, vertical) = if is_row {
            (align(main_align), align(cross_align))
        } else {
            (align(cross_align), align(main_align))
        };
        let mut layout = Layout::fill()
            .direction(direction)
            .padding(Edges::all(padding))
            .gap(gap)
            .align_horizontal(horizontal)
            .align_vertical(vertical);
        let equivalent = Layout::fill()
            .align_vertical(vertical)
            .gap(gap)
            .align_horizontal(horizontal)
            .padding(Edges::all(padding))
            .direction(direction);
        prop_assert_eq!(layout, equivalent);
        if overflow {
            layout = layout.overflow_x().overflow_y();
        }

        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget))?;
        let (first_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let (second_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 4)));
        let (last_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let first = core.add_boxed(Box::new(first_widget))?;
        let second = core.add_boxed(Box::new(second_widget))?;
        let last = core.add_boxed(Box::new(last_widget))?;
        core.set_children(parent, vec![first, second, last])?;
        attach_root_child(&mut core, parent)?;
        core.set_layout_of(parent, layout)?;
        core.set_layout_of(first, Layout::column().fixed_width(3).fixed_height(2))?;
        let second_layout = if is_row {
            Layout::column().flex_horizontal(1).fixed_height(4)
        } else {
            Layout::column().fixed_width(5).flex_vertical(1)
        };
        core.set_layout_of(second, second_layout)?;
        core.set_layout_of(
            last,
            if remove_last {
                Layout::column().fixed_width(2).fixed_height(1).none()
            } else {
                Layout::column().fixed_width(2).fixed_height(1)
            },
        )?;
        core.set_hidden(last, hide_last)?;
        core.update_layout(Size::new(screen_w, screen_h))?;

        let visible = [first, second, last]
            .into_iter()
            .filter(|node| !core.nodes[*node].hidden && core.nodes[*node].layout.display == Display::Block)
            .collect::<Vec<_>>();
        let content = core.nodes[parent].content_size;
        let available_main = direction.main_size(content);
        let available_cross = direction.cross_size(content);
        let children_main = visible.iter().fold(0u32, |total, node| {
            total.saturating_add(direction.main_size(core.nodes[*node].rect.expanse()))
        });
        let gap_total = gap.saturating_mul(u32::try_from(visible.len().saturating_sub(1)).unwrap_or(u32::MAX));
        let group_main = children_main.saturating_add(gap_total);
        let mut expected_main = align_offset(group_main, available_main, align(main_align));

        for node in visible {
            let rect = core.nodes[node].rect;
            let actual_main = if is_row { rect.tl.x } else { rect.tl.y };
            let actual_cross = if is_row { rect.tl.y } else { rect.tl.x };
            prop_assert_eq!(actual_main, expected_main);
            prop_assert_eq!(
                actual_cross,
                align_offset(direction.cross_size(rect.expanse()), available_cross, align(cross_align))
            );
            expected_main = expected_main
                .saturating_add(direction.main_size(rect.expanse()))
                .saturating_add(gap);
        }
    }

    #[test]
    fn generated_invalid_layouts_never_mutate_nodes(kind in 0u8..3) {
        let mut core = Core::new();
        let node = core.create_detached(LayoutWidget(Layout::column()))?;
        let before = core.nodes[node].layout;
        let invalid = match kind {
            0 => Layout::column().flex_vertical(0),
            1 => Layout::column().min_height(u32::MAX).max_height(u32::MAX - 1),
            _ => Layout::column().padding(Edges::new(u32::MAX, 0, 1, 0)),
        };
        prop_assert!(matches!(core.set_layout_of(node, invalid), Err(Error::InvalidLayout(_))));
        prop_assert_eq!(core.nodes[node].layout, before);
    }
}

#[test]
fn invalid_layout_mutations_are_rejected_without_change() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let before = core.nodes[parent].layout;

    let invalid = [
        Layout::column().width(Sizing::Flex(0)),
        Layout::column().min_width(2).max_width(1),
        Layout::column().padding(Edges::new(0, u32::MAX, 0, 1)),
    ];
    for layout in invalid {
        assert!(matches!(
            core.set_layout_of(parent, layout),
            Err(Error::InvalidLayout(_))
        ));
        assert_eq!(core.nodes[parent].layout, before);
    }
    Ok(())
}

#[test]
fn invalid_widget_layouts_are_rejected_before_publication() -> Result<()> {
    let mut core = Core::new();
    let node_count = core.nodes.len();
    let invalid = Layout::row().flex_horizontal(0);

    assert!(matches!(
        core.create_detached(LayoutWidget(invalid)),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes.len(), node_count);

    let target = core.create_detached(LayoutWidget(Layout::column()))?;
    let before = core.nodes[target].layout;
    assert!(matches!(
        core.replace_subtree(target, LayoutWidget(invalid)),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes[target].layout, before);
    Ok(())
}

#[test]
fn positions_monotonic_main() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    let child3 = core.add_boxed(Box::new(c3))?;
    core.set_children(parent, vec![child1, child2, child3])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::row().flex_horizontal(1).gap(1);
    })?;
    core.update_layout(Size::new(20, 5))?;
    let p1 = core.nodes[child1].rect.tl.x;
    let p2 = core.nodes[child2].rect.tl.x;
    let p3 = core.nodes[child3].rect.tl.x;
    assert!(p1 <= p2 && p2 <= p3);
    Ok(())
}

#[test]
fn no_overlaps_with_min_expansion() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::row().flex_horizontal(1);
    })?;
    core.with_layout_of(child1, |layout| {
        layout.width = Sizing::Flex(1);
        layout.min_width = Some(10);
    })?;
    core.with_layout_of(child2, |layout| {
        layout.width = Sizing::Flex(1);
        layout.min_width = Some(10);
    })?;
    core.update_layout(Size::new(5, 5))?;
    let first = &core.nodes[child1];
    let second = &core.nodes[child2];
    assert_eq!(second.rect.tl.x, first.rect.tl.x + first.rect.w);
    Ok(())
}

#[test]
fn overflow_positions_consistent() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::row().flex_horizontal(1).gap(1);
    })?;
    core.update_layout(Size::new(5, 5))?;
    assert_eq!(core.nodes[child2].rect.tl.x, 5);
    Ok(())
}

#[test]
fn canvas_clamped_at_least_view() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(1, 1));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill();
    })?;
    core.update_layout(Size::new(5, 5))?;
    let node = &core.nodes[child];
    assert_eq!(node.canvas, Size::new(5, 5));
    Ok(())
}

#[test]
fn offset_clamped_when_canvas_shrinks() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill();
    })?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 15, y: 15 };
    }
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
    *canvas.lock().unwrap() = Size::new(12, 12);
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 2, y: 2 });
    Ok(())
}

#[test]
fn offset_clamped_when_view_grows() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill();
    })?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 15, y: 15 };
    }
    core.update_layout(Size::new(5, 5))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 15, y: 15 });
    core.update_layout(Size::new(10, 10))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
    Ok(())
}

#[test]
fn zero_view_clamps_scroll() -> Result<()> {
    let mut core = Core::new();
    let canvas = Arc::new(Mutex::new(Size::new(10, 10)));
    let canvas_clone = Arc::clone(&canvas);
    let (widget, _) = TestWidget::with_canvas(
        |_c| Measurement::Wrap,
        move |_view, _ctx| *canvas_clone.lock().unwrap(),
    );
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill();
    })?;
    if let Some(node) = core.nodes.get_mut(child) {
        node.scroll = Point { x: 5, y: 5 };
    }
    core.update_layout(Size::new(0, 0))?;
    assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 0 });
    Ok(())
}

#[test]
fn extreme_padding_is_rejected_without_mutation() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(u32::MAX, u32::MAX)));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    let before = core.nodes[child].layout;
    assert!(matches!(
        core.set_layout_of(child, Layout::fill().padding(Edges::all(u32::MAX))),
        Err(Error::InvalidLayout(_))
    ));
    assert_eq!(core.nodes[child].layout, before);
    Ok(())
}

#[test]
fn child_screen_origin_signed() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(20, 10));
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 2)));
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().fixed_width(2).fixed_height(2);
    })?;
    if let Some(node) = core.nodes.get_mut(parent) {
        node.scroll = Point { x: 5, y: 0 };
    }
    core.update_layout(Size::new(10, 10))?;
    let child_view = core.nodes[child].view;
    assert_eq!(child_view.outer.tl.x, -5);
    Ok(())
}

#[test]
fn content_rect_respects_padding() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
    let child = core.add_boxed(Box::new(widget))?;
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::column().padding(Edges::all(1));
    })?;
    core.update_layout(Size::new(20, 20))?;
    let view = core.nodes[child].view;
    assert_eq!(view.content.tl.x, view.outer.tl.x + 1);
    assert_eq!(view.content.tl.y, view.outer.tl.y + 1);
    assert_eq!(view.content.w, view.outer.w.saturating_sub(2));
    assert_eq!(view.content.h, view.outer.h.saturating_sub(2));
    Ok(())
}

#[test]
fn random_tree_no_panics() -> Result<()> {
    let mut core = Core::new();
    let mut rng = StdRng::seed_from_u64(0x5eed);
    let root_child = build_random_tree(&mut core, &mut rng, 3)?;
    attach_root_child(&mut core, root_child)?;
    core.update_layout(Size::new(40, 20))?;

    for node in core.nodes.values() {
        let expected_w = node.rect.w.saturating_sub(node.layout.padding.horizontal());
        let expected_h = node.rect.h.saturating_sub(node.layout.padding.vertical());
        assert_eq!(node.content_size.w, expected_w);
        assert_eq!(node.content_size.h, expected_h);
        assert!(node.canvas.w >= node.content_size.w);
        assert!(node.canvas.h >= node.content_size.h);
        let max_x = node.canvas.w.saturating_sub(node.content_size.w);
        let max_y = node.canvas.h.saturating_sub(node.content_size.h);
        assert!(node.scroll.x <= max_x);
        assert!(node.scroll.y <= max_y);
    }

    for node in core.nodes.values() {
        // For Stack direction, children can overlap, so skip position ordering check
        if node.layout.direction == LayoutDirection::Stack {
            continue;
        }
        let mut last = 0u32;
        for child in &node.children {
            let child = &core.nodes[*child];
            if child.layout.display == Display::None || child.hidden {
                continue;
            }
            let pos = match node.layout.direction {
                LayoutDirection::Row => child.rect.tl.x,
                LayoutDirection::Column => child.rect.tl.y,
                LayoutDirection::Stack => continue,
            };
            assert!(pos >= last);
            last = pos;
        }
    }

    Ok(())
}

fn build_random_tree(core: &mut Core, rng: &mut StdRng, depth: usize) -> Result<NodeId> {
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let node = core.add_boxed(Box::new(widget))?;
    let mut layout = if rng.random_bool(0.5) {
        Layout::row()
    } else {
        Layout::column()
    };
    if rng.random_bool(0.6) {
        layout.width = Sizing::Flex(rng.random_range(1..3));
    }
    if rng.random_bool(0.6) {
        layout.height = Sizing::Flex(rng.random_range(1..3));
    }
    layout.padding = Edges::new(
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
    );
    layout.gap = rng.random_range(0..3);
    let width_bounds = [rng.random_range(0..6), rng.random_range(0..6)];
    let height_bounds = [rng.random_range(0..6), rng.random_range(0..6)];
    layout.min_width = rng
        .random_bool(0.3)
        .then_some(width_bounds[0].min(width_bounds[1]));
    layout.max_width = rng
        .random_bool(0.3)
        .then_some(width_bounds[0].max(width_bounds[1]));
    layout.min_height = rng
        .random_bool(0.3)
        .then_some(height_bounds[0].min(height_bounds[1]));
    layout.max_height = rng
        .random_bool(0.3)
        .then_some(height_bounds[0].max(height_bounds[1]));
    core.with_layout_of(node, |l| {
        *l = layout;
    })?;

    if depth > 0 {
        let child_count = rng.random_range(0..=3);
        if child_count > 0 {
            let mut children = Vec::new();
            for _ in 0..child_count {
                children.push(build_random_tree(core, rng, depth - 1)?);
            }
            core.set_children(node, children)?;
        }
    }

    Ok(node)
}

#[test]
fn stack_children_overlap() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::stack();
    })?;
    core.update_layout(Size::new(50, 50))?;

    // Both children should be at the same position (0, 0) by default
    assert_eq!(core.nodes[child1].rect.tl.x, 0);
    assert_eq!(core.nodes[child1].rect.tl.y, 0);
    assert_eq!(core.nodes[child2].rect.tl.x, 0);
    assert_eq!(core.nodes[child2].rect.tl.y, 0);

    // Parent content size should be the max of children
    let parent_node = &core.nodes[parent];
    assert_eq!(parent_node.content_size, Size::new(10, 10));
    Ok(())
}

#[test]
fn sequential_alignment_controls_group_and_cross_axes() -> Result<()> {
    for (direction, screen, expected) in [
        (
            Direction::Row,
            Size::new(20, 10),
            [Point { x: 10, y: 4 }, Point { x: 15, y: 3 }],
        ),
        (
            Direction::Column,
            Size::new(10, 20),
            [Point { x: 3, y: 12 }, Point { x: 2, y: 16 }],
        ),
    ] {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget))?;
        let (first_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let (second_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 4)));
        let first = core.add_boxed(Box::new(first_widget))?;
        let second = core.add_boxed(Box::new(second_widget))?;
        core.set_children(parent, vec![first, second])?;
        attach_root_child(&mut core, parent)?;
        core.set_layout_of(
            parent,
            Layout::fill()
                .direction(direction)
                .gap(2)
                .align_horizontal(if direction == Direction::Row {
                    Align::End
                } else {
                    Align::Center
                })
                .align_vertical(if direction == Direction::Column {
                    Align::End
                } else {
                    Align::Center
                }),
        )?;

        core.update_layout(screen)?;
        assert_eq!(core.nodes[first].rect.tl, expected[0]);
        assert_eq!(core.nodes[second].rect.tl, expected[1]);
    }
    Ok(())
}

#[test]
fn locate_node_prefers_topmost_stack_child() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::stack();
    })?;
    core.update_layout(Size::new(50, 50))?;

    let hit = core.locate_node(core.root, Point { x: 1, y: 1 })?;
    assert_eq!(hit, Some(child2));
    Ok(())
}

#[test]
fn stack_with_center_alignment() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::fill().direction(Direction::Stack).align_center();
    })?;
    core.update_layout(Size::new(50, 50))?;

    // Child should be centered in the 50x50 parent
    let child_node = &core.nodes[child];
    assert_eq!(child_node.rect.tl.x, 20); // (50 - 10) / 2
    assert_eq!(child_node.rect.tl.y, 20); // (50 - 10) / 2
    Ok(())
}

#[test]
fn stack_with_end_alignment() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::End)
            .align_vertical(Align::End);
    })?;
    core.update_layout(Size::new(50, 50))?;

    // Child should be at the end (bottom-right)
    let child_node = &core.nodes[child];
    assert_eq!(child_node.rect.tl.x, 40); // 50 - 10
    assert_eq!(child_node.rect.tl.y, 40); // 50 - 10
    Ok(())
}

#[test]
fn stack_multiple_children_centered() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(20, 20)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.add_boxed(Box::new(c1))?;
    let child2 = core.add_boxed(Box::new(c2))?;
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::fill().direction(Direction::Stack).align_center();
    })?;
    core.update_layout(Size::new(50, 50))?;

    // Both children should be centered independently
    let c1_node = &core.nodes[child1];
    let c2_node = &core.nodes[child2];
    assert_eq!(c1_node.rect.tl.x, 15); // (50 - 20) / 2
    assert_eq!(c1_node.rect.tl.y, 15);
    assert_eq!(c2_node.rect.tl.x, 20); // (50 - 10) / 2
    assert_eq!(c2_node.rect.tl.y, 20);
    Ok(())
}

#[test]
fn set_children_detaches_from_previous_parent() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent_a = core.add_boxed(Box::new(parent_widget))?;
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent_b = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget))?;

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
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget))?;
    core.set_children(parent, vec![child])?;

    let err = core.set_children(child, vec![parent]).unwrap_err();
    assert!(matches!(err, Error::WouldCreateCycle { .. }));
    Ok(())
}

#[test]
fn set_children_rejects_duplicates() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget))?;
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget))?;

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
        core.replace_widget_keep_children(
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
            .try_reconcile(
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
            .try_reconcile(
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
        keyed.try_reconcile(
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
        keyed.try_reconcile(
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
        keyed.try_reconcile(
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
        keyed.try_reconcile(
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
            .try_reconcile(
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
