use std::{
    any::TypeId,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

use proptest::{prelude::*, test_runner::TestCaseResult};
use rand::{RngExt, SeedableRng, rngs::StdRng};

use super::{
    layout_driver::{
        LayoutPass, allocate_flex_shares, clamp_outer, clamp_scroll, constraint_for_axis,
        refresh_layouts,
    },
    *,
};
use crate::{
    Context,
    core::context::CoreContext,
    error::{Error, Result},
    geom::{Point, Size},
    layout::{
        Align, CanvasContext, Constraint, Direction, Direction as LayoutDirection, Display, Edges,
        Layout, MeasureConstraints, Measurement, Sizing,
    },
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

struct MountFailWidget;

impl Widget for MountFailWidget {
    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        Err(Error::Invalid("mount failed".into()))
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
    let parent = core.create_detached(simple_widget());
    let current = core.create_detached(simple_widget());
    let sibling = core.create_detached(simple_widget());
    let focused = core.create_detached(FocusableWidget);
    let focus_fallback = core.create_detached(FocusableWidget);
    let mouse_capture = core.create_detached(simple_widget());
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

fn property_nodes(core: &mut Core) -> Vec<Option<NodeId>> {
    (0..PROPERTY_NODE_COUNT)
        .map(|_| Some(core.create_detached(simple_widget())))
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
        let mut nodes = property_nodes(&mut core);
        prop_assert!(core.validate_invariants().is_ok());

        for mutation in mutations {
            apply_tree_mutation(&mut core, &mut nodes, &mutation)?;
        }
    }
}

#[test]
fn validate_invariants_accepts_laid_out_tree() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget());
    let child = core.create_detached(simple_widget());
    core.set_children(parent, vec![child])?;
    attach_root_child(&mut core, parent)?;
    core.update_layout(Size::new(10, 10))?;
    core.validate_invariants()
}

#[test]
fn validate_invariants_rejects_detached_focus() {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget);
    core.set_focus(child);

    let error = core
        .validate_invariants()
        .expect_err("detached focus should fail validation");
    assert!(matches!(error, Error::Invariant(_)));
}

#[test]
fn validate_invariants_rejects_missing_child_link() -> Result<()> {
    let mut core = Core::new();
    let parent = core.create_detached(simple_widget());
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());

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
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());

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
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());
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
    let child = core.create_detached(simple_widget());

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
    let child = core.create_detached(simple_widget());

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
    core.set_focus(nodes.focused);

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
    core.mouse_capture = Some(nodes.mouse_capture);

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
fn clamp_outer_min_greater_than_max() {
    let mut layout = Layout::column();
    layout.min_width = Some(10);
    layout.max_width = Some(5);
    assert_eq!(clamp_outer(Size::new(8, 1), layout), Size::new(5, 1));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let parent = core.add_boxed(Box::new(widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 3)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
    let child3 = core.add_boxed(Box::new(c3));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 1)));
    let child = core.add_boxed(Box::new(child_widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 4)));
    let child = core.add_boxed(Box::new(child_widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
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
    let child1 = core.add_boxed(Box::new(child1_widget));
    let child2 = core.add_boxed(Box::new(child2_widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
    let child3 = core.add_boxed(Box::new(c3));
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

proptest! {
    #[test]
    fn flex_shares_preserve_remaining_space(remaining in 0u32..1000, weights in prop::collection::vec(0u32..20, 0..12)) {
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
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let child = core.add_boxed(Box::new(child_widget));

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
        core.set_hidden(child, hidden);

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
}

#[test]
fn flex_weight_zero_clamped() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
    core.set_children(parent, vec![child1, child2])?;
    attach_root_child(&mut core, parent)?;
    core.with_layout_of(parent, |layout| {
        *layout = Layout::row().flex_horizontal(1);
    })?;
    core.with_layout_of(child1, |layout| {
        layout.width = Sizing::Flex(0);
    })?;
    core.with_layout_of(child2, |layout| {
        layout.width = Sizing::Flex(0);
    })?;
    core.update_layout(Size::new(10, 5))?;
    assert_eq!(core.nodes[child1].rect.w, 5);
    assert_eq!(core.nodes[child2].rect.w, 5);
    Ok(())
}

#[test]
fn positions_monotonic_main() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
    let child3 = core.add_boxed(Box::new(c3));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let child = core.add_boxed(Box::new(widget));
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
fn extreme_padding_layout_does_not_panic() -> Result<()> {
    let mut core = Core::new();
    let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(u32::MAX, u32::MAX)));
    let child = core.add_boxed(Box::new(widget));
    attach_root_child(&mut core, child)?;
    core.with_layout_of(child, |layout| {
        *layout = Layout::fill().padding(Edges::all(u32::MAX));
    })?;

    core.update_layout(Size::new(u32::MAX, u32::MAX))?;

    let node = &core.nodes[child];
    assert_eq!(node.content_size, Size::ZERO);
    assert_eq!(node.canvas, Size::ZERO);
    Ok(())
}

#[test]
fn child_screen_origin_signed() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) =
        TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(20, 10));
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 2)));
    let child = core.add_boxed(Box::new(child_widget));
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
    let child = core.add_boxed(Box::new(widget));
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
    let node = core.add_boxed(Box::new(widget));
    let mut layout = if rng.random_bool(0.5) {
        Layout::row()
    } else {
        Layout::column()
    };
    if rng.random_bool(0.6) {
        layout.width = Sizing::Flex(rng.random_range(0..3));
    }
    if rng.random_bool(0.6) {
        layout.height = Sizing::Flex(rng.random_range(0..3));
    }
    layout.padding = Edges::new(
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
        rng.random_range(0..3),
    );
    layout.gap = rng.random_range(0..3);
    if rng.random_bool(0.3) {
        layout.min_width = Some(rng.random_range(0..6));
    }
    if rng.random_bool(0.3) {
        layout.max_width = Some(rng.random_range(0..6));
    }
    if rng.random_bool(0.3) {
        layout.min_height = Some(rng.random_range(0..6));
    }
    if rng.random_bool(0.3) {
        layout.max_height = Some(rng.random_range(0..6));
    }
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
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
fn locate_node_prefers_topmost_stack_child() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child = core.add_boxed(Box::new(child_widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child = core.add_boxed(Box::new(child_widget));
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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(20, 20)));
    let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
    let child1 = core.add_boxed(Box::new(c1));
    let child2 = core.add_boxed(Box::new(c2));
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
    let parent_a = core.add_boxed(Box::new(parent_widget));
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent_b = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget));

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
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget));
    core.set_children(parent, vec![child])?;

    let err = core.set_children(child, vec![parent]).unwrap_err();
    assert!(matches!(err, Error::WouldCreateCycle { .. }));
    Ok(())
}

#[test]
fn set_children_rejects_duplicates() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.add_boxed(Box::new(parent_widget));
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.add_boxed(Box::new(child_widget));

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
    let parent = core.create_detached(parent_widget);
    let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let child = core.create_detached(child_widget);

    core.attach(parent, child)?;
    let err = core.attach(child, parent).unwrap_err();
    assert!(matches!(err, Error::WouldCreateCycle { .. }));
    Ok(())
}

#[test]
fn remove_subtree_recovers_focus_to_next() -> Result<()> {
    let mut core = Core::new();
    let first = core.create_detached(FocusableWidget);
    let second = core.create_detached(FocusableWidget);
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

    core.set_focus(first);
    core.remove_subtree(first)?;

    assert_eq!(core.focus, Some(second));
    Ok(())
}

#[test]
fn remove_subtree_recovers_focus_to_prev() -> Result<()> {
    let mut core = Core::new();
    let first = core.create_detached(FocusableWidget);
    let second = core.create_detached(FocusableWidget);
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

    core.set_focus(second);
    core.remove_subtree(second)?;

    assert_eq!(core.focus, Some(first));
    Ok(())
}

#[test]
fn detach_clears_mouse_capture() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget);
    core.attach(core.root, child)?;
    core.mouse_capture = Some(child);

    core.detach(child)?;

    assert!(core.mouse_capture.is_none());
    assert!(core.nodes.get(child).is_some());
    assert!(core.nodes[child].parent.is_none());
    Ok(())
}

#[test]
fn remove_subtree_clears_mouse_capture() -> Result<()> {
    let mut core = Core::new();
    let child = core.create_detached(FocusableWidget);
    core.attach(core.root, child)?;
    core.mouse_capture = Some(child);

    core.remove_subtree(child)?;

    assert!(core.mouse_capture.is_none());
    assert!(core.nodes.get(child).is_none());
    Ok(())
}

#[test]
fn keyed_children_require_unique_keys() -> Result<()> {
    let mut core = Core::new();
    let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    let parent = core.create_detached(parent_widget);
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
    let parent = core.create_detached(parent_widget);
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
    let parent = core.create_detached(parent_widget);
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
