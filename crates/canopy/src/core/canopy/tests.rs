use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use futures::{StreamExt, executor::block_on};

use super::*;
use crate::{
    Context, ViewContext,
    commands::{CommandNode, CommandSpec},
    derive_commands,
    error::{Error, NodeOperationKind, Result},
    event::{Event, key, mouse},
    geom::{Direction, Point, RectI32},
    layout::Layout,
    path::Path,
    render::{NopBackend, Render},
    state::NodeName,
    testing::{
        backend::TestRender,
        ttree::{Ba, BaLa, BaLb, OutcomeTarget, R, get_state, reset_state, run_ttree},
    },
    widget::{EventOutcome, Widget},
};

use crate::core::world::test_support::assert_error_context;

static POLL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn synchronous_automation_request_rejects_ui_thread() {
    let canopy = Canopy::new();
    let error = canopy
        .automation_handle()
        .request(|_| Ok(()))
        .expect_err("UI-thread request should be rejected");
    assert!(matches!(error, Error::RunLoop(_)));
}

#[test]
fn automation_service_is_bounded_and_requests_redraw() -> Result<()> {
    let mut canopy = Canopy::new();
    let handle = canopy.automation_handle();
    let count = Arc::new(AtomicUsize::new(0));
    for _ in 0..=AUTOMATION_SERVICE_BUDGET {
        let count = Arc::clone(&count);
        handle.submit(Box::new(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        }))?;
    }

    canopy.render_pending = false;
    assert_eq!(canopy.service_automation(), AUTOMATION_SERVICE_BUDGET);
    assert_eq!(count.load(Ordering::Relaxed), AUTOMATION_SERVICE_BUDGET);
    assert!(canopy.render_pending);
    assert_eq!(canopy.service_automation(), 1);
    assert_eq!(count.load(Ordering::Relaxed), AUTOMATION_SERVICE_BUDGET + 1);
    Ok(())
}

#[test]
fn automation_submission_applies_backpressure() -> Result<()> {
    let canopy = Canopy::new();
    let handle = canopy.automation_handle();
    for _ in 0..AUTOMATION_QUEUE_CAPACITY {
        handle.submit(Box::new(|_| {}))?;
    }
    assert!(matches!(
        handle.submit(Box::new(|_| {})),
        Err(Error::RunLoop(_))
    ));
    Ok(())
}

#[test]
fn cross_thread_automation_request_completes_via_service_path() -> Result<()> {
    let mut canopy = Canopy::new();
    let mut events = canopy
        .event_rx
        .take()
        .expect("test should own framework events");
    let handle = canopy.automation_handle();
    let worker = thread::spawn(move || handle.request(|_| Ok(42)));

    assert!(matches!(block_on(events.next()), Some(Event::Wake)));
    assert_eq!(canopy.service_automation(), 1);
    assert_eq!(worker.join().expect("request worker should not panic")?, 42);
    Ok(())
}

fn canopy_with_binding_order(inputs: [char; 2]) -> Result<Canopy> {
    let mut canopy = Canopy::new();
    for input in inputs {
        canopy.eval_script(&format!(
            "canopy.bind_with({input:?}, {{}}, function() canopy.set_mode(\"next\") end)"
        ))?;
    }
    Ok(canopy)
}

#[test]
fn help_and_diagnostics_use_canonical_binding_order() -> Result<()> {
    let forward = canopy_with_binding_order(['a', 'b'])?;
    let reverse = canopy_with_binding_order(['b', 'a'])?;
    let help_inputs = |canopy: &Canopy| {
        canopy
            .help_snapshot()
            .bindings
            .iter()
            .map(|binding| binding.input.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(help_inputs(&forward), ["a", "b"]);
    assert_eq!(help_inputs(&reverse), ["a", "b"]);

    for canopy in [&forward, &reverse] {
        let diagnostics = canopy.diagnostic_dump(canopy.root_id());
        let a = diagnostics.find(" mode=\"\" a ").expect("a binding");
        let b = diagnostics.find(" mode=\"\" b ").expect("b binding");
        assert!(a < b);
    }
    Ok(())
}

#[test]
fn pending_script_finalization_failure_is_atomic_and_retryable() -> Result<()> {
    let mut canopy = Canopy::new();
    assert_eq!(canopy.script_api_state(), ScriptApiState::Open);
    assert!(canopy.script_api().is_err());
    let host = canopy.script_host.clone();
    let first = host.compile("return 1")?;
    let second = host.compile("return 2")?;
    host.inject_finalize_failure(script::FinalizeStep::PendingScript(1));

    canopy
        .finalize_api()
        .expect_err("pending script load should fail");
    assert_eq!(canopy.script_api_state(), ScriptApiState::Open);
    assert!(canopy.script_api().is_err());

    canopy.finalize_api()?;
    assert_eq!(canopy.script_api_state(), ScriptApiState::Ready);
    let root = canopy.root_id();
    assert_eq!(
        host.execute(&mut canopy, root, first, None)?,
        commands::ArgValue::Int(1)
    );
    assert_eq!(
        host.execute(&mut canopy, root, second, None)?,
        commands::ArgValue::Int(2)
    );
    Ok(())
}

#[test]
fn every_finalization_checkpoint_is_atomic_and_retryable() -> Result<()> {
    let steps = [
        script::FinalizeStep::SurfacePrepared,
        script::FinalizeStep::DeclarationsValidated,
        script::FinalizeStep::DefaultBindingsCompiled,
        script::FinalizeStep::StartupScriptsCompiled,
        script::FinalizeStep::RuntimeBuilt,
        script::FinalizeStep::PendingScript(1),
        script::FinalizeStep::BeforePublish,
    ];

    for step in steps {
        let mut canopy = Canopy::new();
        canopy.register_default_bindings("fault_owner", "canopy.log('default')")?;
        canopy.register_startup_script("fault_startup", "function setup() end")?;
        let host = canopy.script_host.clone();
        let first = host.compile("return 1")?;
        let second = host.compile("return 2")?;
        host.inject_finalize_failure(step);

        canopy
            .finalize_api()
            .expect_err(&format!("{step:?} should fail"));
        assert_eq!(canopy.script_api_state(), ScriptApiState::Open, "{step:?}");
        assert!(canopy.script_api().is_err(), "{step:?}");
        assert_eq!(host.script_ids().len(), 2, "{step:?}");

        canopy.finalize_api()?;
        assert_eq!(canopy.script_api_state(), ScriptApiState::Ready, "{step:?}");
        assert_eq!(host.script_ids().len(), 4, "{step:?}");
        let root = canopy.root_id();
        assert_eq!(
            host.execute(&mut canopy, root, first, None)?,
            commands::ArgValue::Int(1)
        );
        assert_eq!(
            host.execute(&mut canopy, root, second, None)?,
            commands::ArgValue::Int(2)
        );
    }
    Ok(())
}

pub struct PollWidget;

#[derive_commands]
impl PollWidget {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for PollWidget {
    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
        POLL_COUNT.fetch_add(1, Ordering::SeqCst);
        None
    }
}

pub struct StaticWidget;

#[derive_commands]
impl StaticWidget {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for StaticWidget {
    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }
}

pub struct FailRenderWidget;

impl Widget for FailRenderWidget {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, _rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Err(Error::Invalid("render failed".into()))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("fail_render")
    }
}

pub struct CaptureWidget {
    drags: usize,
}

#[derive_commands]
impl CaptureWidget {
    pub fn new() -> Self {
        Self { drags: 0 }
    }
}

impl Widget for CaptureWidget {
    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event {
            match mouse_event.action {
                mouse::Action::Down if mouse_event.button == mouse::Button::Left => {
                    ctx.capture_mouse()?;
                    return Ok(EventOutcome::Handle);
                }
                mouse::Action::Drag if mouse_event.button == mouse::Button::Left => {
                    self.drags = self.drags.saturating_add(1);
                    return Ok(EventOutcome::Handle);
                }
                mouse::Action::Up if mouse_event.button == mouse::Button::Left => {
                    ctx.release_mouse()?;
                    return Ok(EventOutcome::Handle);
                }
                _ => {}
            }
        }
        Ok(EventOutcome::Ignore)
    }
}

fn set_outcome<T: Any + OutcomeTarget>(core: &mut Core, id: NodeId, outcome: EventOutcome) {
    let _ignored = core.with_widget_mut(id, |w, _| {
        let any = w as &mut dyn Any;
        if let Some(node) = any.downcast_mut::<T>() {
            node.set_outcome(outcome);
        }
    });
}

fn capture_drag_count(core: &mut Core, id: NodeId) -> usize {
    core.with_widget_mut(id, |w, _| {
        let any = w as &mut dyn Any;
        any.downcast_mut::<CaptureWidget>()
            .map(|widget| widget.drags)
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

fn make_mouse_event(core: &Core, node_id: NodeId) -> mouse::MouseEvent {
    let loc = core
        .nodes
        .get(node_id)
        .map(|n| {
            let tl = n.view.outer.tl;
            Point {
                x: tl.x.max(0) as u32,
                y: tl.y.max(0) as u32,
            }
        })
        .unwrap_or_default();
    mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: loc,
    }
}

#[test]
fn render_errors_include_operation_node_and_path() -> Result<()> {
    let mut render = TestRender::new();
    let mut canopy = Canopy::new();
    canopy
        .core
        .replace_subtree(canopy.core.root, FailRenderWidget)?;
    canopy.set_root_size(Size::new(10, 2))?;
    let node_id = canopy.core.root;
    let path = canopy.core.node_path(canopy.core.root, node_id).to_string();

    let error = canopy
        .render(&mut render)
        .expect_err("render should include node context");

    assert!(matches!(
        error,
        Error::NodeOperation {
            kind: NodeOperationKind::Render,
            ..
        }
    ));
    assert_error_context(&error, "render", node_id, &path);
    Ok(())
}

#[test]
fn mouse_move_does_not_request_render() -> Result<()> {
    let mut canopy = Canopy::new();
    let app_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(StaticWidget::new()))?;
    canopy.core.set_layout_of(app_id, Layout::fill())?;
    canopy.set_root_size(Size::new(10, 6))?;

    let mut render = TestRender::new();
    canopy.render(&mut render)?;
    assert!(!canopy.render_if_pending(&mut render)?);

    let event = mouse::MouseEvent {
        action: mouse::Action::Moved,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: Point { x: 1, y: 1 },
    };
    canopy.event(Event::Mouse(event))?;
    assert!(!canopy.render_if_pending(&mut render)?);
    Ok(())
}

#[test]
fn mouse_capture_routes_drag_outside() -> Result<()> {
    let mut canopy = Canopy::new();
    let app_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(CaptureWidget::new()))?;
    canopy.core.set_layout_of(app_id, Layout::fill())?;
    canopy.set_root_size(Size::new(10, 6))?;

    let mut render = TestRender::new();
    canopy.render(&mut render)?;

    let down = make_mouse_event(&canopy.core, app_id);
    canopy.event(Event::Mouse(down))?;

    let drag = mouse::MouseEvent {
        action: mouse::Action::Drag,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: Point { x: 50, y: 50 },
    };
    canopy.event(Event::Mouse(drag))?;

    assert_eq!(capture_drag_count(&mut canopy.core, app_id), 1);

    let up = mouse::MouseEvent {
        action: mouse::Action::Up,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: Point { x: 50, y: 50 },
    };
    canopy.event(Event::Mouse(up))?;

    Ok(())
}

#[test]
fn mouse_routing_clears_a_stale_internal_capture() -> Result<()> {
    let mut canopy = Canopy::new();
    let stale = canopy.core.create_detached(CaptureWidget::new())?;
    canopy.core.remove_subtree(stale)?;
    canopy.core.mouse_capture = Some(stale);

    let event = mouse::MouseEvent {
        action: mouse::Action::Moved,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: Point::zero(),
    };
    canopy.event(Event::Mouse(event))?;
    assert_eq!(canopy.core.mouse_capture, None);
    Ok(())
}

#[test]
fn set_widget_resets_initialization() -> Result<()> {
    POLL_COUNT.store(0, Ordering::SeqCst);
    let mut canopy = Canopy::new();
    let node_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(PollWidget::new()))?;
    canopy.set_root_size(Size::new(10, 10))?;

    let mut render = TestRender::new();
    render.render(&mut canopy)?;
    assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 1);

    canopy.core.replace_subtree(node_id, PollWidget::new())?;
    render.render(&mut canopy)?;
    assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn registered_native_modules_appear_once_in_the_api() -> Result<()> {
    use ruau::module;

    use crate::commands::declaration;

    let mut builder = module::Builder::new("demo_module");
    builder.constant(
        "answer",
        module::Binding::library("demo_module", declaration::Type::Number)
            .doc("A registered native constant."),
        42i64,
    );
    let demo = builder.build().expect("demo module builds");

    let mut canopy = Canopy::new();
    canopy.register_script_module(demo)?;
    canopy.finalize_api()?;

    let api = canopy.script_api()?;
    assert_eq!(api.matches("declare demo_module").count(), 1);
    assert!(api.contains("answer: number"));
    Ok(())
}

#[test]
fn tbindings() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.eval_script(
            r#"
            canopy.bind_with("a", {}, function() ba_la.c_leaf() end)
            canopy.bind_with("r", {}, function() r.c_root() end)
            canopy.bind_with("x", { path = "ba/" }, function() r.c_root() end)
            "#,
        )?;

        c.core.set_focus(tree.a_a)?;
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->ignore", "ba_la.c_leaf()"]);

        reset_state();
        c.key(None, 'r')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

        reset_state();
        c.core.set_focus(tree.a)?;
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba@key->ignore", "ba_la.c_leaf()"]);

        reset_state();
        c.core.set_focus(tree.a_a)?;
        c.key(None, 'x')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->ignore", "r.c_root()"]);

        reset_state();
        c.core.set_focus(tree.root)?;
        c.key(None, 'x')?;
        let s = get_state();
        assert_eq!(s.path, vec!["r@key->ignore"]);

        Ok(())
    })?;
    Ok(())
}

#[test]
fn input_mode_binding_target_switches_modes() -> Result<()> {
    let mut canopy = Canopy::new();
    canopy.eval_script(r#"canopy.bind_with("i", {}, function() canopy.set_mode("insert") end)"#)?;

    canopy.key(None, 'i')?;

    assert_eq!(canopy.input_mode(), "insert");
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| entry.phase == RoutePhase::BindingExecution)
    );
    Ok(())
}

#[test]
fn route_trace_records_unhandled_key_pipeline() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        c.key(None, 'z')?;
        let phases = c
            .route_trace()
            .iter()
            .map(|entry| entry.phase)
            .collect::<Vec<_>>();

        assert!(phases.contains(&RoutePhase::Target));
        assert!(phases.contains(&RoutePhase::WidgetEvent));
        assert!(phases.contains(&RoutePhase::Bubble));
        assert!(phases.contains(&RoutePhase::Unhandled));
        assert!(c.diagnostic_dump(tree.a_a).contains("route trace:"));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn register_default_bindings_is_idempotent_for_identical_scripts() -> Result<()> {
    run_ttree(|c, _, _| {
        c.register_default_bindings("r", "canopy.log(\"once\")")?;
        c.register_default_bindings("r", "canopy.log(\"once\")")?;

        let err = c
            .register_default_bindings("r", "canopy.log(\"twice\")")
            .unwrap_err();
        assert!(matches!(err, error::Error::Invalid(_)));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn tkey() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.root)?;
        set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["r@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->ignore", "ba@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(
            s.path,
            vec!["ba_la@key->ignore", "ba@key->ignore", "r@key->handle"]
        );
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a)?;
        set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a)?;
        set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba@key->ignore", "r@key->handle"]);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(
            s.path,
            vec![
                "ba@key->ignore",
                "r@key->handle",
                "ba@key->ignore",
                "r@key->ignore"
            ]
        );
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_b)?;
        set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Ignore);
        set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(
            s.path,
            vec!["ba_lb@key->ignore", "ba@key->ignore", "r@key->handle"]
        );
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_b)?;
        set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_lb@key->ignore", "ba@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_b)?;
        set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_lb@key->handle"]);
        Ok(())
    })?;

    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_b)?;
        set_outcome::<BaLb>(&mut c.core, tree.a_b, EventOutcome::Handle);
        set_outcome::<Ba>(&mut c.core, tree.a, EventOutcome::Handle);
        c.key(None, 'a')?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_lb@key->handle"]);
        Ok(())
    })?;

    Ok(())
}

#[test]
fn tmouse() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        c.core.set_focus(tree.root)?;
        set_outcome::<R>(&mut c.core, tree.root, EventOutcome::Handle);
        tr.render(c)?;
        let evt = make_mouse_event(&c.core, tree.a_a);
        c.mouse(None, evt)?;
        let s = get_state();
        assert_eq!(
            s.path,
            vec!["ba_la@mouse->ignore", "ba@mouse->ignore", "r@mouse->handle"]
        );
        Ok(())
    })?;

    run_ttree(|c, mut tr, tree| {
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        tr.render(c)?;
        let evt = make_mouse_event(&c.core, tree.a_a);
        c.mouse(None, evt)?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
        Ok(())
    })?;

    run_ttree(|c, mut tr, tree| {
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        tr.render(c)?;
        let evt = make_mouse_event(&c.core, tree.a_a);
        c.mouse(None, evt)?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
        Ok(())
    })?;

    run_ttree(|c, mut tr, tree| {
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        tr.render(c)?;
        let evt = make_mouse_event(&c.core, tree.a_a);
        c.mouse(None, evt)?;
        let s = get_state();
        assert_eq!(s.path, vec!["ba_la@mouse->handle"]);
        Ok(())
    })?;

    Ok(())
}

#[test]
fn tresize() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        let size: u32 = 100;
        let half = i32::try_from(size / 2).expect("size fits i32");
        tr.render(c)?;
        assert_eq!(
            c.core.nodes[tree.root].view.outer,
            RectI32::new(0, 0, size, size)
        );
        assert_eq!(
            c.core.nodes[tree.a].view.outer,
            RectI32::new(0, 0, size / 2, size)
        );
        assert_eq!(
            c.core.nodes[tree.b].view.outer,
            RectI32::new(half, 0, size / 2, size)
        );

        c.set_root_size(Size::new(50, 50))?;
        tr.render(c)?;
        assert_eq!(c.core.nodes[tree.b].view.outer, RectI32::new(25, 0, 25, 50));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn trender() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        tr.render(c)?;
        assert!(!tr.buf_empty());

        tr.render(c)?;
        assert!(tr.buf_empty());
        tr.render(c)?;
        tr.render(c)?;
        tr.render(c)?;

        tr.render(c)?;
        assert!(tr.buf_empty());

        c.core.set_focus(tree.a_a)?;
        tr.render(c)?;
        assert!(tr.buf_empty());

        c.core.focus_next(c.core.root)?;
        tr.render(c)?;
        assert!(tr.buf_empty());

        c.core.focus_prev(c.core.root)?;
        tr.render(c)?;
        assert!(tr.buf_empty());

        tr.render(c)?;
        assert!(tr.buf_empty());

        Ok(())
    })?;

    Ok(())
}

#[test]
fn focus_path() -> Result<()> {
    run_ttree(|c, _, _tree| {
        assert_eq!(c.core.focus_path(c.core.root), Path::empty());
        c.core.focus_next(c.core.root)?;
        assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));
        c.core.focus_next(c.core.root)?;
        assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));
        c.core.focus_next(c.core.root)?;
        assert_eq!(
            c.core.focus_path(c.core.root),
            Path::new(&["r", "ba", "ba_la"])
        );
        Ok(())
    })?;
    Ok(())
}

#[test]
fn focus_next() -> Result<()> {
    run_ttree(|c, _, tree| {
        assert!(!c.core.is_focused(tree.root));
        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.root));

        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.a));

        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.a_a));
        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.a_b));
        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.b));

        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.b_a));
        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.b_b));

        c.core.focus_next(c.core.root)?;
        assert!(c.core.is_focused(tree.root));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn focus_prev() -> Result<()> {
    run_ttree(|c, _, tree| {
        assert!(!c.core.is_focused(tree.root));
        c.core.focus_prev(c.core.root)?;
        assert!(c.core.is_focused(tree.b_b));

        c.core.focus_prev(c.core.root)?;
        assert!(c.core.is_focused(tree.b_a));

        c.core.focus_prev(c.core.root)?;
        assert!(c.core.is_focused(tree.b));

        c.core.set_focus(tree.root)?;
        c.core.focus_prev(c.core.root)?;
        assert!(c.core.is_focused(tree.b_b));

        Ok(())
    })?;
    Ok(())
}

#[test]
fn tshift_right() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        tr.render(c)?;
        c.core.set_focus(tree.a_a)?;
        c.core.focus_dir(c.core.root, Direction::Right)?;
        assert!(c.core.is_focused(tree.b_a));
        c.core.focus_dir(c.core.root, Direction::Right)?;
        assert!(c.core.is_focused(tree.b_a));

        c.core.set_focus(tree.a_b)?;
        c.core.focus_dir(c.core.root, Direction::Right)?;
        assert!(c.core.is_focused(tree.b_b));
        c.core.focus_dir(c.core.root, Direction::Right)?;
        assert!(c.core.is_focused(tree.b_b));
        Ok(())
    })?;

    Ok(())
}

#[test]
fn tfoci() -> Result<()> {
    run_ttree(|c, _, tree| {
        assert_eq!(c.core.focus_path(c.core.root), Path::empty());

        assert!(!c.core.is_on_focus_path(tree.root));
        assert!(!c.core.is_on_focus_path(tree.a));

        c.core.set_focus(tree.a_a)?;
        assert!(c.core.is_on_focus_path(tree.root));
        assert!(c.core.is_on_focus_path(tree.a));
        assert!(!c.core.is_on_focus_path(tree.b));
        assert_eq!(
            c.core.focus_path(c.core.root),
            Path::new(&["r", "ba", "ba_la"])
        );

        c.core.set_focus(tree.a)?;
        assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r", "ba"]));

        c.core.set_focus(tree.root)?;
        assert_eq!(c.core.focus_path(c.core.root), Path::new(&["r"]));

        c.core.set_focus(tree.b_a)?;
        assert_eq!(
            c.core.focus_path(c.core.root),
            Path::new(&["r", "bb", "bb_la"])
        );
        Ok(())
    })?;

    Ok(())
}

#[test]
fn tkey_no_render() -> Result<()> {
    struct N;

    impl CommandNode for N {
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for N {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }

        fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
            r.text("any", ctx.view().outer_rect_local().line(0)?, "<n>")
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            let outcome = match event {
                Event::Key(_) => EventOutcome::Consume,
                _ => EventOutcome::Ignore,
            };
            Ok(outcome)
        }

        fn name(&self) -> NodeName {
            NodeName::convert("n")
        }
    }

    let mut tr = TestRender::new();
    let mut canopy = Canopy::new();
    canopy.add_commands::<N>()?;
    canopy.core.replace_subtree(canopy.core.root, N)?;

    canopy.set_root_size(Size::new(10, 1))?;
    canopy.core.set_focus(canopy.core.root)?;
    canopy.render(&mut tr)?;
    assert!(!tr.buf_empty());
    let prev_buf = canopy.termbuf.clone().expect("missing termbuf");
    tr.text.clear();

    canopy.key(None, 'a')?;
    canopy.render(&mut tr)?;
    let next_buf = canopy.termbuf.clone().expect("missing termbuf");
    assert_eq!(prev_buf.cells, next_buf.cells);
    Ok(())
}

#[test]
fn zero_size_child_ok() -> Result<()> {
    struct Child;

    #[derive_commands]
    impl Child {}

    impl Widget for Child {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("child")
        }
    }

    struct Parent;

    #[derive_commands]
    impl Parent {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for Parent {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("parent")
        }
    }

    let size = Size::new(5, 1);
    let mut cr = NopBackend::new();
    let mut canopy = Canopy::new();
    canopy
        .core
        .replace_subtree(canopy.core.root, Parent::new())?;
    let child = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(Child))?;
    canopy
        .core
        .set_layout_of(child, Layout::column().fixed_width(0).fixed_height(0))?;

    canopy.set_root_size(size)?;
    canopy.render(&mut cr)?;
    Ok(())
}

#[test]
fn visible_render_limits_reject_sizes_before_publication() -> Result<()> {
    let mut canopy = Canopy::new();
    assert!(matches!(
        canopy.set_root_size(Size::new(2049, 1)),
        Err(Error::RenderWidthLimit { .. })
    ));
    assert_eq!(canopy.root_size, None);

    canopy.set_render_limits(RenderLimits::new(4, 4, 15))?;
    assert!(matches!(
        canopy.set_root_size(Size::new(4, 4)),
        Err(Error::RenderCellLimit { .. })
    ));
    assert_eq!(canopy.root_size, None);

    let accepted = RenderLimits::new(4, 4, 16);
    canopy.set_render_limits(accepted)?;
    canopy.set_root_size(Size::new(4, 4))?;
    assert!(matches!(
        canopy.set_render_limits(RenderLimits::new(3, 4, 16)),
        Err(Error::RenderWidthLimit { .. })
    ));
    assert_eq!(canopy.render_limits, accepted);
    Ok(())
}
