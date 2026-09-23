use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
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
    Context, FocusDirection, ViewContext,
    commands::{CommandNode, CommandSpec, CommandStatus},
    core::{
        inputmap::InputSpec,
        keyroute::{BindingVerdict, KeyExpectation, RouteOutcome, RouteWinner},
        world::test_support::assert_error_context,
    },
    derive_commands,
    error::{Error, NodeOperationKind, Result, ScriptErrorKind},
    event::{Event, key, mouse},
    geom::{PointI32, RectI32},
    layout::{Edges, Layout},
    path::Path,
    render::{NopBackend, Render},
    script::LuauFunctionId,
    state::NodeName,
    testing::{
        backend::TestRender,
        ttree::{Ba, BaLa, BaLb, OutcomeTarget, R, get_state, reset_state, run_ttree},
    },
    widget::{EventOutcome, Widget},
};

static POLL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Build an application with no registrations.
fn app() -> Canopy {
    CanopyBuilder::new()
        .build()
        .expect("an empty application builds")
}

/// Build an application after one registration callback.
fn app_with(configure: impl FnOnce(&mut Setup) -> Result<()> + 'static) -> Canopy {
    CanopyBuilder::new()
        .configure(configure)
        .build()
        .expect("the application builds")
}

/// Build an application that registers the `test.clear` intent.
fn clear_intent_app() -> Canopy {
    app_with(|setup| setup.register_intent(inputmap::IntentSpec::new("test.clear", "Clear")?))
}

/// Install an application binding on a running application, as a script's
/// `canopy.bind` does.
fn bind(
    canopy: &mut Canopy,
    input: impl Into<InputSpec>,
    options: inputmap::BindingOptions,
    target: inputmap::BindingAction,
) -> Result<inputmap::BindingId> {
    let (id, removed) = canopy.core.input_map.bind(input.into(), options, target)?;
    canopy.release_removed_bindings(removed);
    Ok(id)
}

/// Install an application command binding on a running application.
fn bind_command(
    canopy: &mut Canopy,
    input: impl Into<InputSpec>,
    options: inputmap::BindingOptions,
    command: commands::CommandCall,
) -> Result<inputmap::BindingId> {
    bind(
        canopy,
        input,
        options,
        inputmap::BindingAction::Command(command),
    )
}

/// Install an application intent binding on a running application.
fn bind_intent(
    canopy: &mut Canopy,
    input: impl Into<InputSpec>,
    options: inputmap::BindingOptions,
    action: inputmap::IntentName,
) -> Result<inputmap::BindingId> {
    bind(
        canopy,
        input,
        options,
        inputmap::BindingAction::Intent(action),
    )
}

#[test]
fn synchronous_automation_request_rejects_ui_thread() {
    let canopy = app();
    let error = canopy
        .automation_handle()
        .request(|_| Ok(()))
        .expect_err("UI-thread request should be rejected");
    assert!(matches!(error, Error::Driver(_)));
}

#[test]
fn read_only_automation_service_is_bounded_without_redraw() -> Result<()> {
    let mut canopy = app();
    let handle = canopy.automation_handle();
    let count = Arc::new(AtomicUsize::new(0));
    for _ in 0..=AUTOMATION_SERVICE_BUDGET {
        let count = Arc::clone(&count);
        handle.submit(Box::new(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
        }))?;
    }

    canopy.core.changes = crate::ChangeSet::default();
    assert_eq!(canopy.service_automation(), AUTOMATION_SERVICE_BUDGET);
    assert_eq!(count.load(Ordering::Relaxed), AUTOMATION_SERVICE_BUDGET);
    assert!(!canopy.core.changes.is_pending());
    assert_eq!(canopy.service_automation(), 1);
    assert_eq!(count.load(Ordering::Relaxed), AUTOMATION_SERVICE_BUDGET + 1);
    Ok(())
}

#[test]
fn automation_submission_applies_backpressure() -> Result<()> {
    let canopy = app();
    let handle = canopy.automation_handle();
    for _ in 0..AUTOMATION_QUEUE_CAPACITY {
        handle.submit(Box::new(|_| {}))?;
    }
    assert!(matches!(
        handle.submit(Box::new(|_| {})),
        Err(Error::Driver(_))
    ));
    Ok(())
}

#[test]
fn cross_thread_automation_request_completes_via_service_path() -> Result<()> {
    let mut canopy = app();
    let mut events = canopy
        .event_rx
        .take()
        .expect("test should own framework events");
    let handle = canopy.automation_handle();
    let worker = thread::spawn(move || handle.request(|_| Ok(42)));

    assert!(matches!(block_on(events.next()), Some(AdapterEvent::Wake)));
    assert_eq!(canopy.service_automation(), 1);
    assert_eq!(worker.join().expect("request worker should not panic")?, 42);
    Ok(())
}

fn canopy_with_binding_order(inputs: [char; 2]) -> Result<Canopy> {
    let mut canopy = app();
    for input in inputs {
        canopy.eval_script(&format!(
            "canopy.bind({input:?}, {{ description = \"Change mode\" }}, function() canopy.set_mode(\"next\") end)"
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
            .available_bindings(None)
            .expect("root binding snapshot")
            .bindings
            .iter()
            .map(|binding| binding.input.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(help_inputs(&forward), ["a", "b"]);
    assert_eq!(help_inputs(&reverse), ["a", "b"]);

    for canopy in [&forward, &reverse] {
        let diagnostics = canopy.diagnostic_dump(canopy.root_id());
        let a = diagnostics.find(" a  ->").expect("a binding");
        let b = diagnostics.find(" b  ->").expect("b binding");
        assert!(a < b);
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
    fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
        POLL_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

pub struct StaticWidget;

#[derive_commands]
impl StaticWidget {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for StaticWidget {}

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
    let _ignored = core.with_widget_dyn_mut(id, |w, _| {
        let any = w as &mut dyn Any;
        if let Some(node) = any.downcast_mut::<T>() {
            node.set_outcome(outcome);
        }
    });
}

fn capture_drag_count(core: &mut Core, id: NodeId) -> usize {
    core.with_widget_dyn_mut(id, |w, _| {
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
        .map(|n| n.view.outer.tl)
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
    let mut canopy = app();
    canopy
        .core
        .replace_subtree(canopy.core.root, FailRenderWidget)?;
    canopy.set_screen_size(Size::new(10, 2))?;
    let node_id = canopy.core.root;
    let path = canopy.core.path_of(canopy.core.root, node_id).to_string();

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
fn ignored_mouse_callback_conservatively_requests_render() -> Result<()> {
    let mut canopy = app();
    let app_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(StaticWidget::new()))?;
    canopy.core.set_layout_of(app_id, Layout::fill())?;
    canopy.set_screen_size(Size::new(10, 6))?;

    let mut render = TestRender::new();
    canopy.render(&mut render)?;
    assert!(!canopy.render_if_pending(&mut render)?);

    let event = mouse::MouseEvent {
        action: mouse::Action::Moved,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32 { x: 1, y: 1 },
    };
    canopy.event(&Event::Mouse(event))?;
    assert!(canopy.render_if_pending(&mut render)?);
    assert!(!canopy.render_if_pending(&mut render)?);
    Ok(())
}

#[test]
fn mouse_capture_routes_drag_outside() -> Result<()> {
    let mut canopy = app();
    let app_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(CaptureWidget::new()))?;
    canopy.core.set_layout_of(app_id, Layout::fill())?;
    canopy.set_screen_size(Size::new(10, 6))?;

    let mut render = TestRender::new();
    canopy.render(&mut render)?;

    let down = make_mouse_event(&canopy.core, app_id);
    canopy.event(&Event::Mouse(down))?;

    let drag = mouse::MouseEvent {
        action: mouse::Action::Drag,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: PointI32 { x: 50, y: 50 },
    };
    canopy.event(&Event::Mouse(drag))?;

    assert_eq!(capture_drag_count(&mut canopy.core, app_id), 1);

    let up = mouse::MouseEvent {
        action: mouse::Action::Up,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: PointI32 { x: 50, y: 50 },
    };
    canopy.event(&Event::Mouse(up))?;

    Ok(())
}

#[test]
fn mouse_routing_clears_a_stale_internal_capture() -> Result<()> {
    let mut canopy = app();
    let stale = canopy.core.create_detached(CaptureWidget::new())?;
    canopy.core.remove_subtree(stale)?;
    canopy.core.mouse_capture = Some(stale);

    let event = mouse::MouseEvent {
        action: mouse::Action::Moved,
        button: mouse::Button::None,
        modifiers: key::Empty,
        location: PointI32::default(),
    };
    canopy.event(&Event::Mouse(event))?;
    assert_eq!(canopy.core.mouse_capture, None);
    Ok(())
}

#[test]
fn set_widget_resets_initialization() -> Result<()> {
    POLL_COUNT.store(0, Ordering::SeqCst);
    let mut canopy = app();
    let node_id = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(PollWidget::new()))?;
    canopy.set_screen_size(Size::new(10, 10))?;

    let mut render = TestRender::new();
    canopy.render(&mut render)?;
    assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 1);

    canopy.core.replace_subtree(node_id, PollWidget::new())?;
    canopy.render(&mut render)?;
    assert_eq!(POLL_COUNT.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn tbindings() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.eval_script(
            r#"
            canopy.bind("a", { description = "Leaf command" }, function() ba_la.c_leaf() end)
            canopy.bind("r", { description = "Root command" }, function() r.c_root() end)
            canopy.bind("x", { path = "ba/", description = "Root fallback" }, function() r.c_root() end)
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
        assert_eq!(
            s.path,
            vec!["ba_la@key->ignore", "ba@key->ignore", "r.c_root()"]
        );

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
fn framework_command_bindings_share_route_resolution_and_event_scope() -> Result<()> {
    run_ttree(|c, _, tree| {
        let group = inputmap::FrameworkBindingGroup::new("test.modal");
        let (binding, _) = c.core.input_map.bind(
            'h'.into(),
            inputmap::BindingOptions {
                path: Some("/r/**/".parse()?),
                tier: inputmap::BindingTier::Framework(group),
                description: "Framework root command".to_string(),
                source: None,
                phase: Some(inputmap::BindingPhase::BeforeWidget),
            },
            inputmap::BindingAction::Command(R::call_c_root()),
        )?;
        c.core
            .input_map
            .set_modal_bindings(Some(crate::ModalBindings::Framework {
                group,
                intents: &[],
            }));
        c.core.set_focus(tree.a_a)?;

        let snapshot = c.available_bindings(None)?;
        assert_eq!(snapshot.framework_group, Some(group));
        assert_eq!(snapshot.bindings.len(), 1);
        assert_eq!(snapshot.bindings[0].id, binding);

        reset_state();
        c.key(None, 'h')?;

        assert_eq!(get_state().path, ["r.c_root()"]);
        assert!(c.route_trace().iter().any(|entry| {
            entry.kind == RouteTraceKind::RunBinding && entry.detail == "Framework root command"
        }));
        c.core.input_map.set_modal_bindings(None);
        Ok(())
    })
}

#[test]
fn explicit_binding_phases_override_the_same_selector_and_change_route_trace() -> Result<()> {
    run_ttree(|c, _, tree| {
        c.core.set_focus(tree.a_a)?;
        for phase in [
            inputmap::BindingPhase::BeforeWidget,
            inputmap::BindingPhase::AfterWidget,
        ] {
            bind_command(
                c,
                'h',
                inputmap::BindingOptions {
                    path: Some("/r/**/".parse()?),
                    tier: inputmap::BindingTier::Default,
                    description: "Root action".into(),
                    source: None,
                    phase: Some(phase),
                },
                R::call_c_root(),
            )?;
            let snapshot = c.available_bindings(None)?;
            let binding = snapshot
                .bindings
                .iter()
                .find(|binding| binding.input == 'h')
                .unwrap();
            assert_eq!(binding.phase, phase);
            assert_eq!(binding.path_filter, "/r/**/");
            reset_state();
            c.key(None, 'h')?;
            let phases = c
                .route_trace()
                .iter()
                .map(|entry| entry.kind)
                .collect::<Vec<_>>();
            if phase == inputmap::BindingPhase::BeforeWidget {
                assert_eq!(get_state().path, ["r.c_root()"]);
                assert!(phases.contains(&RouteTraceKind::BeforeWidgetBinding));
                assert!(!phases.contains(&RouteTraceKind::Widget));
            } else {
                assert_eq!(get_state().path, ["ba_la@key->ignore", "r.c_root()"]);
                assert!(phases.contains(&RouteTraceKind::AfterWidgetBinding));
                assert!(phases.contains(&RouteTraceKind::Widget));
                assert!(!phases.contains(&RouteTraceKind::BeforeWidgetBinding));
            }
        }
        Ok(())
    })
}

/// Build a left-button press over `node`.
fn click_on(core: &Core, node: NodeId) -> mouse::MouseEvent {
    make_mouse_event(core, node)
}

/// Options for one mouse binding on the test tree.
fn mouse_options(path: &str, phase: inputmap::BindingPhase) -> Result<inputmap::BindingOptions> {
    Ok(inputmap::BindingOptions {
        path: Some(path.parse()?),
        tier: inputmap::BindingTier::Default,
        description: "Click action".into(),
        source: None,
        phase: Some(phase),
    })
}

#[test]
fn a_mouse_binding_runs_in_the_phase_it_declares() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        c.render(&mut tr)?;
        let click = inputmap::InputSpec::Mouse(click_on(&c.core, tree.a_a).into());
        for phase in [
            inputmap::BindingPhase::BeforeWidget,
            inputmap::BindingPhase::AfterWidget,
        ] {
            bind_command(
                c,
                click,
                mouse_options("/r/ba/ba_la/", phase)?,
                BaLa::call_c_leaf(),
            )?;
            reset_state();
            c.mouse(None, click_on(&c.core, tree.a_a))?;
            let phases = c
                .route_trace()
                .iter()
                .map(|entry| entry.kind)
                .collect::<Vec<_>>();
            if phase == inputmap::BindingPhase::BeforeWidget {
                // The early binding takes the click instead of the widget.
                assert_eq!(get_state().path, ["ba_la.c_leaf()"]);
                assert!(phases.contains(&RouteTraceKind::BeforeWidgetBinding));
                assert!(!phases.contains(&RouteTraceKind::Widget));
            } else {
                assert_eq!(get_state().path, ["ba_la@mouse->ignore", "ba_la.c_leaf()"]);
                assert!(phases.contains(&RouteTraceKind::AfterWidgetBinding));
                assert!(!phases.contains(&RouteTraceKind::BeforeWidgetBinding));
            }
        }
        Ok(())
    })
}

#[test]
fn one_winner_decides_each_route_node_and_phases_stay_local() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        c.render(&mut tr)?;
        let click = inputmap::InputSpec::Mouse(click_on(&c.core, tree.a_a).into());
        // An ancestor's early binding is early only at the ancestor. The leaf's
        // widget still sees the click first, because the route reaches the leaf
        // before the ancestor exists as a route node at all.
        bind_command(
            c,
            click,
            mouse_options("/r/", inputmap::BindingPhase::BeforeWidget)?,
            R::call_c_root(),
        )?;
        reset_state();
        c.mouse(None, click_on(&c.core, tree.a_a))?;
        assert_eq!(
            get_state().path,
            ["ba_la@mouse->ignore", "ba@mouse->ignore", "r.c_root()"]
        );

        // A more specific path wins at the leaf, and its phase is the one the
        // leaf uses. The ancestor binding never runs, because the route ends at
        // the first node that acts.
        bind_command(
            c,
            click,
            mouse_options("/r/**/ba_la/", inputmap::BindingPhase::BeforeWidget)?,
            BaLa::call_c_leaf(),
        )?;
        reset_state();
        c.mouse(None, click_on(&c.core, tree.a_a))?;
        assert_eq!(get_state().path, ["ba_la.c_leaf()"]);

        // A widget that handles the click ends the route before the winning
        // late binding at that node, and no second binding is tried.
        bind_command(
            c,
            click,
            mouse_options("/r/**/ba_la/", inputmap::BindingPhase::AfterWidget)?,
            BaLa::call_c_leaf(),
        )?;
        set_outcome::<BaLa>(&mut c.core, tree.a_a, EventOutcome::Handle);
        reset_state();
        c.mouse(None, click_on(&c.core, tree.a_a))?;
        assert_eq!(get_state().path, ["ba_la@mouse->handle"]);
        Ok(())
    })
}

/// Node that captures the mouse and records what an early binding saw.
struct ClickProbe {
    /// Node-local location of each click a binding ran for, and whether the
    /// probe still held capture when it ran.
    seen: Vec<(PointI32, bool)>,
}

#[derive_commands]
impl ClickProbe {
    /// Record the node-local location of the click that ran this command.
    #[command]
    fn note(&mut self, ctx: &dyn Context, event: mouse::MouseEvent) -> Result<()> {
        self.seen.push((event.location, ctx.has_mouse_capture()));
        Ok(())
    }
}

impl Widget for ClickProbe {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(m) = event
            && m.action == mouse::Action::Down
        {
            ctx.capture_mouse()?;
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("click_probe")
    }
}

#[test]
fn early_mouse_bindings_keep_capture_and_node_local_coordinates() -> Result<()> {
    let mut c = app_with(|setup| setup.add_commands::<ClickProbe>());
    let probe = c.core.create_detached(ClickProbe { seen: Vec::new() })?;
    c.core.set_children(c.core.root, vec![probe])?;
    c.core
        .set_layout_of(c.core.root, Layout::fill().padding(Edges::all(4)))?;
    c.set_screen_size(Size::new(40, 20))?;
    c.core.update_layout(Size::new(40, 20))?;

    let drag = inputmap::InputSpec::Mouse(
        mouse::MouseEvent {
            action: mouse::Action::Drag,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 0, y: 0 },
        }
        .into(),
    );
    bind_command(
        &mut c,
        drag,
        mouse_options("/root/click_probe/", inputmap::BindingPhase::BeforeWidget)?,
        ClickProbe::call_note(),
    )?;

    // The press captures the mouse, so the drag that follows routes to the
    // probe even though it leaves the node.
    c.mouse(
        None,
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 6, y: 6 },
        },
    )?;
    assert_eq!(c.core.mouse_capture, Some(probe));
    c.mouse(
        None,
        mouse::MouseEvent {
            action: mouse::Action::Drag,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: PointI32 { x: 1, y: 9 },
        },
    )?;

    let content = c.core.nodes[probe].view.content.tl;
    let seen = c
        .core
        .with_widget_dyn_mut(probe, |widget, _| {
            (widget as &mut dyn Any)
                .downcast_mut::<ClickProbe>()
                .map(|probe| probe.seen.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    assert_eq!(
        seen,
        [(
            PointI32 {
                x: 1 - content.x,
                y: 9 - content.y,
            },
            true
        )],
        "an early binding sees the node-local location and the capture it ran under"
    );
    assert_eq!(
        c.core.mouse_capture,
        Some(probe),
        "an early binding does not disturb capture"
    );
    Ok(())
}

#[test]
fn an_early_mouse_binding_respects_modal_admission_and_wheel_fallback() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        c.render(&mut tr)?;
        let wheel = inputmap::InputSpec::Mouse(
            mouse::MouseEvent {
                action: mouse::Action::ScrollDown,
                button: mouse::Button::None,
                modifiers: key::Empty,
                location: PointI32 { x: 0, y: 0 },
            }
            .into(),
        );
        // An unbound wheel still reaches its default action rather than an
        // early binding that does not match this route.
        bind_command(
            c,
            wheel,
            mouse_options("/r/bb/**/", inputmap::BindingPhase::BeforeWidget)?,
            R::call_c_root(),
        )?;
        let mut scroll = click_on(&c.core, tree.a_a);
        scroll.action = mouse::Action::ScrollDown;
        scroll.button = mouse::Button::None;
        reset_state();
        c.mouse(None, scroll)?;
        assert!(
            !get_state().path.contains(&"r.c_root()".to_string()),
            "a binding outside the route never runs"
        );

        // A framework group blocks application bindings whatever their phase.
        let group = inputmap::FrameworkBindingGroup::new("test.modal");
        bind_command(
            c,
            inputmap::InputSpec::Mouse(click_on(&c.core, tree.a_a).into()),
            mouse_options("/r/**/", inputmap::BindingPhase::BeforeWidget)?,
            R::call_c_root(),
        )?;
        c.core
            .input_map
            .set_modal_bindings(Some(crate::ModalBindings::Framework {
                group,
                intents: &[],
            }));
        reset_state();
        c.mouse(None, click_on(&c.core, tree.a_a))?;
        assert!(
            !get_state().path.contains(&"r.c_root()".to_string()),
            "a framework group blocks an early application binding"
        );
        c.core.input_map.set_modal_bindings(None);
        Ok(())
    })
}

/// Node whose command is eligible only while it says so.
#[derive(Default)]
struct Gated {
    /// Whether the command reports itself eligible.
    enabled: bool,
    /// Whether the command fails once invoked.
    fail: bool,
    /// Times the command ran.
    runs: usize,
}

#[derive_commands]
impl Gated {
    fn eligibility(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(if self.enabled {
            CommandStatus::Enabled
        } else {
            CommandStatus::Disabled("not now".into())
        })
    }

    #[command(enabled = "eligibility")]
    fn act(&mut self, ctx: &dyn Context) -> Result<()> {
        self.runs += 1;
        assert!(
            ctx.current_event().is_some(),
            "a routed command runs inside the event scope"
        );
        if self.fail {
            Err(Error::Invalid("action failed".into()))
        } else {
            Ok(())
        }
    }
}

impl Widget for Gated {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("gated")
    }
}

/// Build a root and one child, both gated, with the child focused.
fn gated_pair() -> Result<(Canopy, NodeId, NodeId)> {
    let mut canopy = app_with(|setup| setup.add_commands::<Gated>());
    canopy.core.replace_subtree(
        canopy.core.root,
        Gated {
            enabled: true,
            ..Gated::default()
        },
    )?;
    let root = canopy.core.root;
    let child = canopy.core.create_detached(Gated::default())?;
    canopy.core.set_children(root, vec![child])?;
    canopy.core.set_focus(child)?;
    Ok((canopy, root, child))
}

/// Return how many times each gated node ran its command.
fn gated_runs(canopy: &mut Canopy, nodes: [NodeId; 2]) -> [usize; 2] {
    nodes.map(|node| {
        canopy
            .core
            .with_widget_dyn_mut(node, |widget, _| {
                (widget as &mut dyn Any)
                    .downcast_mut::<Gated>()
                    .map_or(0, |gated| gated.runs)
            })
            .unwrap_or(0)
    })
}

/// Options for one gated binding.
fn gated_options(path: &str) -> Result<inputmap::BindingOptions> {
    Ok(inputmap::BindingOptions {
        path: Some(path.parse()?),
        tier: inputmap::BindingTier::Default,
        description: "Act".into(),
        source: None,
        phase: Some(inputmap::BindingPhase::AfterWidget),
    })
}

#[test]
fn a_disabled_declarative_winner_is_consumed_without_running_or_bubbling() -> Result<()> {
    let (mut canopy, root, child) = gated_pair()?;
    bind_command(
        &mut canopy,
        'g',
        gated_options("/gated/gated/")?,
        Gated::call_act().with_target(commands::CommandTarget::Exact(child)),
    )?;
    bind_command(
        &mut canopy,
        'g',
        gated_options("/gated/")?,
        Gated::call_act().with_target(commands::CommandTarget::Exact(root)),
    )?;

    canopy.key(None, 'g')?;
    assert_eq!(
        gated_runs(&mut canopy, [root, child]),
        [0, 0],
        "a disabled winner neither runs nor lets an ancestor act in its place"
    );
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| entry.detail == "binding disabled: not now"),
        "the trace names the reason"
    );
    assert!(
        canopy.core.current_event().is_none(),
        "the skipped binding restored the event scope"
    );

    // The check reads eligibility again rather than trusting the last frame.
    canopy.core.with_widget_dyn_mut(child, |widget, _| {
        if let Some(gated) = (widget as &mut dyn Any).downcast_mut::<Gated>() {
            gated.enabled = true;
        }
    })?;
    canopy.key(None, 'g')?;
    assert_eq!(gated_runs(&mut canopy, [root, child]), [0, 1]);
    assert!(canopy.core.current_event().is_none());
    Ok(())
}

/// Focusable widget whose key handler and command fail.
struct Faulty {
    /// Whether the failure is a runtime failure rather than the widget's own.
    fatal: bool,
}

#[derive_commands]
impl Faulty {
    /// Fail as an application does.
    #[command]
    fn fail(&self) -> Result<()> {
        Err(Error::App("command failed".into()))
    }
}

/// Build a root holding one focused faulty child.
fn faulty_app(fatal: bool) -> Result<(Canopy, NodeId)> {
    let mut canopy = app_with(|setup| setup.add_commands::<Faulty>());
    let child = canopy.core.create_detached(Faulty { fatal })?;
    let root = canopy.core.root;
    canopy.core.set_children(root, vec![child])?;
    canopy.core.set_focus(child)?;
    Ok((canopy, child))
}

impl Widget for Faulty {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        match event {
            Event::Key(_) if self.fatal => Err(Error::Internal("handler broke".into())),
            Event::Key(_) => Err(Error::App("handler failed".into())),
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn key_outcome(&self, _key: key::Key, _ctx: &dyn ViewContext) -> EventOutcome {
        EventOutcome::Handle
    }

    fn name(&self) -> NodeName {
        NodeName::convert("faulty")
    }
}

#[test]
fn a_widget_handler_failure_is_a_notice_unless_it_is_a_runtime_failure() -> Result<()> {
    let (mut canopy, child) = faulty_app(false)?;
    canopy.key(None, 'x')?;
    let notice = canopy
        .notices()
        .last()
        .expect("the handler failure is a notice");
    assert_eq!(notice.source, crate::NoticeSource::Widget);
    assert_eq!(notice.node, Some(child));
    assert_eq!(notice.message, "handler failed");
    assert_eq!(
        canopy.route_trace().last().map(|entry| entry.kind),
        Some(RouteTraceKind::Notice)
    );

    canopy.core.with_widget_dyn_mut(child, |widget, _| {
        if let Some(faulty) = (widget as &mut dyn Any).downcast_mut::<Faulty>() {
            faulty.fatal = true;
        }
    })?;
    assert!(
        canopy.key(None, 'x').is_err(),
        "a runtime failure in a handler stays fatal"
    );
    assert_eq!(canopy.notices().len(), 1);
    Ok(())
}

#[test]
fn a_script_call_still_raises_a_failing_command() -> Result<()> {
    let (mut canopy, _child) = faulty_app(false)?;
    assert!(
        canopy
            .eval_script(r#"canopy.call_focus("faulty::fail")"#)
            .is_err(),
        "a script call raises the failure"
    );
    assert!(
        canopy.notices().is_empty(),
        "a script call records no notice"
    );

    // A key a script sends routes as input, so its binding's failure is a
    // notice the script can read.
    canopy.eval_script(
        r#"
        canopy.bind("g", { description = "Fail", phase = "before_widget" }, command.faulty.fail())
        canopy.send_key("g")
        local notices = canopy.notices()
        canopy.assert(#notices == 1, "one notice")
        canopy.assert(notices[1].source == "binding", "from a binding")
        canopy.assert(notices[1].kind == "command_exec", "a command failure")
        canopy.assert(notices[1].message == "command failed", notices[1].message)
        canopy.assert(notices[1].node ~= nil, "on a node")
        "#,
    )?;
    Ok(())
}

#[test]
fn binding_failures_become_notices_and_still_restore_the_event_scope() -> Result<()> {
    let (mut canopy, root, child) = gated_pair()?;
    canopy.core.with_widget_dyn_mut(child, |widget, _| {
        if let Some(gated) = (widget as &mut dyn Any).downcast_mut::<Gated>() {
            gated.enabled = true;
            gated.fail = true;
        }
    })?;
    bind_command(
        &mut canopy,
        'g',
        gated_options("/gated/gated/")?,
        Gated::call_act().with_target(commands::CommandTarget::Exact(child)),
    )?;
    canopy.key(None, 'g')?;
    assert_eq!(gated_runs(&mut canopy, [root, child]), [0, 1]);
    assert!(canopy.core.current_event().is_none());
    let notice = canopy.notices().last().expect("the failure is a notice");
    assert_eq!(notice.source, crate::NoticeSource::Binding);
    assert_eq!(notice.node, Some(child));
    assert_eq!(notice.kind, ScriptErrorKind::CommandExecution);
    assert_eq!(
        canopy.route_trace().last().map(|entry| entry.kind),
        Some(RouteTraceKind::Notice),
        "the route trace records the notice"
    );
    assert!(canopy.core.notices.shown().is_some(), "the notice is shown");

    // An opaque script callback's failure becomes a notice the same way, and
    // the key dismisses the one shown before it.
    canopy.eval_script(
        r#"canopy.bind("s", { description = "Fail" }, function() error("script failed") end)"#,
    )?;
    canopy.key(None, 's')?;
    assert!(canopy.core.current_event().is_none());
    assert_eq!(canopy.notices().len(), 2);
    let notice = canopy
        .notices()
        .last()
        .expect("the script failure is a notice");
    assert!(
        notice.message.contains("script failed"),
        "{}",
        notice.message
    );

    // Input with no failure dismisses the shown notice and keeps the record.
    canopy.key(None, 'z')?;
    assert!(canopy.core.notices.shown().is_none());
    assert_eq!(canopy.notices().len(), 2);
    Ok(())
}

#[test]
fn mode_binding_target_switches_modes() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(r#"canopy.bind("i", { description = "Insert mode" }, function() canopy.set_mode("insert") end)"#)?;

    canopy.key(None, 'i')?;

    assert_eq!(canopy.mode(), "insert");
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| entry.kind == RouteTraceKind::RunBinding)
    );
    Ok(())
}

#[test]
fn a_transient_mode_takes_the_next_key_before_widgets() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(
        r#"
        canopy.bind("z", { description = "Default z" }, function() canopy.set_mode("default") end)
        canopy.keymap({
            mode = "prefix",
            { key = "y", description = "Prefix y", action = function() canopy.push_mode("after") end },
        })
        canopy.push_mode("prefix", { transient = true })
        local snapshot = canopy.available_bindings()
        canopy.assert(snapshot.transient_mode == "prefix", "the snapshot names the transient mode")
        "#,
    )?;
    let phases = |canopy: &Canopy| {
        canopy
            .route_trace()
            .iter()
            .map(|entry| entry.kind)
            .collect::<Vec<_>>()
    };

    // The mode pops before its binding runs, so the binding can push a mode.
    canopy.key(None, 'y')?;
    assert_eq!(canopy.core.input_map.active_modes(), ["after"]);
    assert!(phases(&canopy).contains(&RouteTraceKind::RunBinding));
    assert!(!phases(&canopy).contains(&RouteTraceKind::Widget));

    // A key the mode does not bind only pops it.
    canopy.set_mode("");
    canopy.push_transient_mode("prefix");
    canopy.key(None, 'z')?;
    assert_eq!(canopy.mode(), "");
    assert!(!phases(&canopy).contains(&RouteTraceKind::RunBinding));
    assert!(!phases(&canopy).contains(&RouteTraceKind::Widget));
    Ok(())
}

#[test]
fn mode_hooks_run_once_for_each_mode_change() -> Result<()> {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    fn count(_context: &mut dyn Context) -> Result<()> {
        RUNS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    let mut canopy = app_with(|setup| {
        setup.register_mode_hook("test.count", count);
        Ok(())
    });
    canopy.set_screen_size(Size::new(10, 4))?;
    let mut backend = NopBackend::new();
    canopy.render(&mut backend)?;
    canopy.render(&mut backend)?;
    assert_eq!(RUNS.load(Ordering::Relaxed), 0, "no mode change yet");

    canopy.push_transient_mode("prefix");
    canopy.render(&mut backend)?;
    canopy.render(&mut backend)?;
    assert_eq!(RUNS.load(Ordering::Relaxed), 1);

    canopy.pop_mode();
    canopy.render(&mut backend)?;
    assert_eq!(RUNS.load(Ordering::Relaxed), 2);
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
            .map(|entry| entry.kind)
            .collect::<Vec<_>>();

        assert!(phases.contains(&RouteTraceKind::Start));
        assert!(phases.contains(&RouteTraceKind::Widget));
        assert!(phases.contains(&RouteTraceKind::Bubble));
        assert!(phases.contains(&RouteTraceKind::Unhandled));
        assert!(c.diagnostic_dump(tree.a_a).contains("route trace:"));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn register_default_bindings_is_idempotent_for_identical_scripts() -> Result<()> {
    let mut setup = Setup::new();
    setup.add_commands::<R>()?;
    setup.register_default_bindings("r", "canopy.log(\"once\")")?;
    setup.register_default_bindings("r", "canopy.log(\"once\")")?;

    let err = setup
        .register_default_bindings("r", "canopy.log(\"twice\")")
        .unwrap_err();
    assert!(matches!(err, error::Error::Invalid(_)));
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
        c.render(&mut tr)?;
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
        c.render(&mut tr)?;
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
        c.render(&mut tr)?;
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

        c.set_screen_size(Size::new(50, 50))?;
        c.render(&mut tr)?;
        assert_eq!(c.core.nodes[tree.b].view.outer, RectI32::new(25, 0, 25, 50));
        Ok(())
    })?;
    Ok(())
}

#[test]
fn trender() -> Result<()> {
    run_ttree(|c, mut tr, tree| {
        c.render(&mut tr)?;
        assert!(!tr.buf_empty());

        c.render(&mut tr)?;
        assert!(tr.buf_empty());
        c.render(&mut tr)?;
        c.render(&mut tr)?;
        c.render(&mut tr)?;

        c.render(&mut tr)?;
        assert!(tr.buf_empty());

        c.core.set_focus(tree.a_a)?;
        c.render(&mut tr)?;
        assert!(tr.buf_empty());

        c.core.focus_next(c.core.root)?;
        c.render(&mut tr)?;
        assert!(tr.buf_empty());

        c.core.focus_prev(c.core.root)?;
        c.render(&mut tr)?;
        assert!(tr.buf_empty());

        c.render(&mut tr)?;
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
        c.render(&mut tr)?;
        c.core.set_focus(tree.a_a)?;
        c.core.focus_move(c.core.root, FocusDirection::Right)?;
        assert!(c.core.is_focused(tree.b_a));
        c.core.focus_move(c.core.root, FocusDirection::Right)?;
        assert!(c.core.is_focused(tree.b_a));

        c.core.set_focus(tree.a_b)?;
        c.core.focus_move(c.core.root, FocusDirection::Right)?;
        assert!(c.core.is_focused(tree.b_b));
        c.core.focus_move(c.core.root, FocusDirection::Right)?;
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
                Event::Key(_) => EventOutcome::Handle,
                _ => EventOutcome::Ignore,
            };
            Ok(outcome)
        }

        fn key_outcome(&self, _key: key::Key, _ctx: &dyn ViewContext) -> EventOutcome {
            EventOutcome::Handle
        }

        fn name(&self) -> NodeName {
            NodeName::convert("n")
        }
    }

    let mut tr = TestRender::new();
    let mut canopy = app_with(|setup| setup.add_commands::<N>());
    canopy.core.replace_subtree(canopy.core.root, N)?;

    canopy.set_screen_size(Size::new(10, 1))?;
    canopy.core.set_focus(canopy.core.root)?;
    canopy.render(&mut tr)?;
    assert!(!tr.buf_empty());
    let prev_buf = canopy.frame.termbuf.clone().expect("missing termbuf");
    tr.text.clear();

    canopy.key(None, 'a')?;
    canopy.render(&mut tr)?;
    let next_buf = canopy.frame.termbuf.clone().expect("missing termbuf");
    assert_eq!(prev_buf.cells, next_buf.cells);
    Ok(())
}

#[test]
fn zero_size_child_ok() -> Result<()> {
    struct Child;

    #[derive_commands]
    impl Child {}

    impl Widget for Child {
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
        fn name(&self) -> NodeName {
            NodeName::convert("parent")
        }
    }

    let size = Size::new(5, 1);
    let mut cr = NopBackend::new();
    let mut canopy = app();
    canopy
        .core
        .replace_subtree(canopy.core.root, Parent::new())?;
    let child = canopy
        .core
        .add_child_to_boxed(canopy.core.root, Box::new(Child))?;
    canopy
        .core
        .set_layout_of(child, Layout::column().fixed_width(0).fixed_height(0))?;

    canopy.set_screen_size(size)?;
    canopy.render(&mut cr)?;
    Ok(())
}

#[test]
fn visible_render_limits_reject_sizes_before_publication() -> Result<()> {
    let mut canopy = app();
    assert!(matches!(
        canopy.set_screen_size(Size::new(2049, 1)),
        Err(Error::RenderWidthLimit { .. })
    ));
    assert_eq!(canopy.frame.screen_size, None);

    let mut canopy = app_with(|setup| {
        setup.set_render_limits(RenderLimits::new(4, 4, 15));
        Ok(())
    });
    assert!(matches!(
        canopy.set_screen_size(Size::new(4, 4)),
        Err(Error::RenderCellLimit { .. })
    ));
    assert_eq!(canopy.frame.screen_size, None);

    let accepted = RenderLimits::new(4, 4, 16);
    let mut canopy = app_with(move |setup| {
        setup.set_render_limits(accepted);
        Ok(())
    });
    canopy.set_screen_size(Size::new(4, 4))?;
    assert_eq!(canopy.frame.render_limits, accepted);
    Ok(())
}

/// Handles and predicts `Handle` for `x`, and ignores every other key.
struct PredictingLeaf;

impl Widget for PredictingLeaf {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(match event {
            Event::Key(key) if *key == 'x' => EventOutcome::Handle,
            _ => EventOutcome::Ignore,
        })
    }

    fn key_outcome(&self, key: key::Key, _context: &dyn ViewContext) -> EventOutcome {
        if key == 'x' {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("predicting_leaf")
    }
}

/// Keeps the default prediction and ignores every key.
struct PlainLeaf;

impl Widget for PlainLeaf {
    fn name(&self) -> NodeName {
        NodeName::convert("plain_leaf")
    }
}

/// Predicts `Ignore` for every key but handles them anyway.
struct OverclaimingLeaf;

impl Widget for OverclaimingLeaf {
    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(if matches!(event, Event::Key(_)) {
            EventOutcome::Handle
        } else {
            EventOutcome::Ignore
        })
    }

    fn name(&self) -> NodeName {
        NodeName::convert("overclaiming_leaf")
    }
}

/// Predicts `Handle` for every key but ignores them.
struct UnderclaimingLeaf;

impl Widget for UnderclaimingLeaf {
    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        Ok(EventOutcome::Ignore)
    }

    fn key_outcome(&self, _key: key::Key, _context: &dyn ViewContext) -> EventOutcome {
        EventOutcome::Handle
    }

    fn name(&self) -> NodeName {
        NodeName::convert("underclaiming_leaf")
    }
}

/// Attach a leaf and focus it.
fn focused_leaf<W: Widget + 'static>(canopy: &mut Canopy, widget: W) -> Result<NodeId> {
    let leaf = canopy.core.create_detached(widget)?;
    canopy.core.attach(canopy.root_id(), leaf)?;
    canopy.core.set_focus(leaf)?;
    Ok(leaf)
}

/// Bind an opaque script callback to one character.
fn bind_key(canopy: &mut Canopy, key: char) -> Result<crate::BindingId> {
    bind_key_phase(canopy, key, crate::BindingPhase::AfterWidget)
}

/// Bind an opaque script callback with an explicit phase.
fn bind_key_phase(
    canopy: &mut Canopy,
    key: char,
    phase: crate::BindingPhase,
) -> Result<crate::BindingId> {
    use crate::core::inputmap::{BindingAction, BindingOptions};
    let (id, _) = canopy.core.input_map.bind(
        InputSpec::Key(key.into()),
        BindingOptions {
            path: None,
            tier: crate::BindingTier::Default,
            description: "Test binding".to_string(),
            source: None,
            phase: Some(phase),
        },
        BindingAction::Script(LuauFunctionId::for_test(1)),
    )?;
    Ok(id)
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "key prediction mismatch")]
fn an_ignored_prediction_that_handles_panics_in_debug() {
    let mut canopy = app();
    focused_leaf(&mut canopy, OverclaimingLeaf).expect("leaf mounted");
    canopy.key(None, 'x').expect("route dispatched");
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "key prediction mismatch")]
fn a_handled_prediction_that_ignores_panics_in_debug() {
    let mut canopy = app();
    focused_leaf(&mut canopy, UnderclaimingLeaf).expect("leaf mounted");
    canopy.key(None, 'x').expect("route dispatched");
}

#[test]
fn a_prediction_mismatch_records_a_route_trace_entry() {
    let mut canopy = app();
    focused_leaf(&mut canopy, OverclaimingLeaf).expect("leaf mounted");
    // A debug build also fails an assertion once the entry is recorded.
    let _outcome = catch_unwind(AssertUnwindSafe(|| canopy.key(None, 'x')));
    assert!(canopy.route_trace().iter().any(|entry| {
        entry.kind == RouteTraceKind::Widget
            && entry.detail == "key prediction mismatch: predicted Ignore, actual Handle"
    }));
}

#[test]
fn explain_key_reports_a_widget_outcome_for_the_focus() -> Result<()> {
    let mut canopy = app();
    let leaf = focused_leaf(&mut canopy, PredictingLeaf)?;
    let explanation = canopy.explain_key(Some(leaf), 'x'.into())?;
    assert_eq!(explanation.focus, leaf);
    assert_eq!(
        explanation.outcome,
        RouteOutcome::Widget {
            node: leaf,
            path: Path::from("/root/predicting_leaf")
        }
    );
    assert_eq!(explanation.steps.len(), 1);
    assert_eq!(explanation.steps[0].widget, EventOutcome::Handle);
    Ok(())
}

#[test]
fn a_default_prediction_passes_the_key_to_an_after_widget_binding() -> Result<()> {
    let mut canopy = app();
    let leaf = focused_leaf(&mut canopy, PlainLeaf)?;
    let id = bind_key(&mut canopy, 'q')?;
    let explanation = canopy.explain_key(Some(leaf), 'q'.into())?;
    assert_eq!(
        explanation.outcome.winner().map(|winner| winner.binding),
        Some(id)
    );
    assert_eq!(explanation.steps[0].node, leaf);
    assert_eq!(explanation.steps[0].widget, EventOutcome::Ignore);
    assert_eq!(
        explanation.steps[0].binding.map(|binding| binding.phase),
        Some(crate::BindingPhase::AfterWidget)
    );
    Ok(())
}

#[test]
fn send_key_checked_delivers_a_matching_binding_from_a_script() -> Result<()> {
    let mut canopy = app();
    // The check runs from evaluated source, so a replay that records the
    // source repeats it.
    canopy.eval_script(
        r#"
        canopy.bind("x", { description = "Switch" }, function() canopy.set_mode("ran") end)
        local active = canopy.available_bindings()
        canopy.send_key_checked("x", { kind = "binding", binding = active.bindings[1].id })
        "#,
    )?;
    assert_eq!(canopy.mode(), "ran");
    Ok(())
}

#[test]
fn send_key_checked_allows_a_widget_to_suppress_an_after_widget_binding() -> Result<()> {
    let mut canopy = app();
    let leaf = focused_leaf(&mut canopy, PredictingLeaf)?;
    bind_key(&mut canopy, 'x')?;

    canopy.send_key_checked('x', KeyExpectation::Widget(leaf))?;

    assert!(
        !canopy
            .route_trace()
            .iter()
            .any(|entry| entry.kind == RouteTraceKind::RunBinding)
    );
    Ok(())
}

#[test]
fn send_key_checked_accepts_an_unhandled_route_at_a_modal_boundary() -> Result<()> {
    let mut canopy = app();
    let modal = focused_leaf(&mut canopy, PredictingLeaf)?;
    canopy.core.open_modal(crate::ModalOptions {
        owner: canopy.root_id(),
        modal,
        initial_focus: modal,
        dim_target: None,
        bindings: crate::ModalBindings::Application,
    })?;

    canopy.send_key_checked('u', KeyExpectation::Unhandled)?;

    assert!(
        !canopy
            .route_trace()
            .iter()
            .any(|entry| entry.kind == RouteTraceKind::RunBinding)
    );
    Ok(())
}

/// Open a modal over a focused leaf and return the leaf.
fn modal_leaf(canopy: &mut Canopy, bindings: crate::ModalBindings) -> Result<NodeId> {
    let modal = focused_leaf(canopy, PredictingLeaf)?;
    canopy.core.open_modal(crate::ModalOptions {
        owner: canopy.root_id(),
        modal,
        initial_focus: modal,
        dim_target: None,
        bindings,
    })?;
    Ok(modal)
}

/// Bind a default-tier `z` and a `prefix` mode `y`, each pushing a mode.
fn bind_prefix_mode(canopy: &mut Canopy) -> Result<()> {
    canopy.eval_script(
        r#"
        canopy.bind("z", { description = "Default z" }, function() canopy.push_mode("default_ran") end)
        canopy.keymap({
            mode = "prefix",
            { key = "y", description = "Prefix y", action = function() canopy.push_mode("after") end },
        })
        "#,
    )?;
    Ok(())
}

#[test]
fn a_transient_mode_under_an_application_modal_takes_the_next_key() -> Result<()> {
    let mut canopy = app();
    modal_leaf(&mut canopy, crate::ModalBindings::Application)?;
    bind_prefix_mode(&mut canopy)?;

    // Discovery, analysis, and routing agree that the mode takes the key.
    canopy.push_transient_mode("prefix");
    let snapshot = canopy.available_bindings(None)?;
    assert_eq!(snapshot.transient_mode.as_deref(), Some("prefix"));
    assert_eq!(
        snapshot
            .bindings
            .iter()
            .map(|binding| binding.description.as_str())
            .collect::<Vec<_>>(),
        ["Prefix y"]
    );
    assert!(matches!(
        canopy.explain_key(None, 'y'.into())?.outcome,
        RouteOutcome::Transient(_)
    ));
    canopy.key(None, 'y')?;
    assert_eq!(
        canopy.core.input_map.active_modes(),
        ["after"],
        "the mode pops before its binding runs"
    );

    // A key the mode does not bind pops it without reaching the default tier.
    canopy.set_mode("");
    canopy.push_transient_mode("prefix");
    assert_eq!(
        canopy.explain_key(None, 'z'.into())?.outcome,
        RouteOutcome::TransientDismiss
    );
    canopy.key(None, 'z')?;
    assert!(canopy.core.input_map.active_modes().is_empty());

    // Once the mode has popped, the default tier is reachable again.
    assert!(matches!(
        canopy.explain_key(None, 'z'.into())?.outcome,
        RouteOutcome::Binding(RouteWinner {
            phase: crate::BindingPhase::AfterWidget,
            ..
        })
    ));
    canopy.key(None, 'z')?;
    assert_eq!(canopy.core.input_map.active_modes(), ["default_ran"]);
    Ok(())
}

#[test]
fn a_checked_transient_key_under_an_application_modal_matches_its_analysis() -> Result<()> {
    let mut canopy = app();
    modal_leaf(&mut canopy, crate::ModalBindings::Application)?;
    bind_prefix_mode(&mut canopy)?;
    canopy.push_transient_mode("prefix");
    let RouteOutcome::Transient(winner) = canopy.explain_key(None, 'y'.into())?.outcome else {
        panic!("expected a transient binding");
    };

    canopy.send_key_checked('y', KeyExpectation::Transient(winner.binding))?;
    assert_eq!(canopy.core.input_map.active_modes(), ["after"]);

    canopy.push_transient_mode("prefix");
    canopy.send_key_checked('q', KeyExpectation::TransientDismiss)?;
    assert_eq!(canopy.core.input_map.active_modes(), ["after"]);
    Ok(())
}

#[test]
fn a_framework_modal_suspends_a_transient_mode() -> Result<()> {
    let mut canopy = app();
    let group = inputmap::FrameworkBindingGroup::new("test.modal");
    modal_leaf(
        &mut canopy,
        crate::ModalBindings::Framework {
            group,
            intents: &[],
        },
    )?;
    bind_prefix_mode(&mut canopy)?;
    canopy.push_transient_mode("prefix");

    let snapshot = canopy.available_bindings(None)?;
    assert_eq!(snapshot.transient_mode, None);
    assert!(snapshot.bindings.is_empty());
    assert_eq!(
        canopy.explain_key(None, 'y'.into())?.outcome,
        RouteOutcome::Unhandled
    );
    canopy.key(None, 'y')?;
    assert_eq!(
        canopy.core.input_map.active_modes(),
        ["prefix"],
        "a suspended mode neither runs nor pops"
    );
    Ok(())
}

#[test]
fn explain_key_reports_before_widget_and_unhandled_outcomes() -> Result<()> {
    let mut canopy = app();
    let leaf = focused_leaf(&mut canopy, PredictingLeaf)?;
    let id = bind_key_phase(&mut canopy, 'b', crate::BindingPhase::BeforeWidget)?;

    let before = canopy.explain_key(Some(leaf), 'b'.into())?;
    assert_eq!(
        before.outcome,
        RouteOutcome::Binding(RouteWinner {
            binding: id,
            node: leaf,
            path: Path::from("/root/predicting_leaf"),
            kind: crate::BindingActionKind::Script,
            phase: crate::BindingPhase::BeforeWidget,
        })
    );

    let unhandled = canopy.explain_key(Some(leaf), 'u'.into())?;
    assert_eq!(unhandled.outcome, RouteOutcome::Unhandled);
    Ok(())
}

#[test]
fn explain_key_matches_the_actual_route() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(
        r#"canopy.bind("q", { description = "Switch" }, function() canopy.set_mode("ran") end)"#,
    )?;
    let id = canopy
        .core
        .input_map
        .bindings()
        .iter()
        .find(|record| record.input == InputSpec::Key('q'.into()))
        .expect("binding")
        .id;

    let explanation = canopy.explain_key(None, 'q'.into())?;
    assert_eq!(
        explanation.outcome,
        RouteOutcome::Binding(RouteWinner {
            binding: id,
            node: canopy.root_id(),
            path: Path::from("/root"),
            kind: crate::BindingActionKind::Script,
            phase: crate::BindingPhase::AfterWidget,
        })
    );

    canopy.key(None, 'q')?;
    assert_eq!(canopy.mode(), "ran");
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| { entry.kind == RouteTraceKind::RunBinding && entry.detail == "Switch" })
    );
    Ok(())
}

#[test]
fn explain_key_reports_transient_binding_and_dismissal() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(
        r#"
        canopy.keymap({
            mode = "prefix",
            { key = "y", description = "Prefix y", action = function() canopy.set_mode("after") end },
        })
        canopy.push_mode("prefix", { transient = true })
        "#,
    )?;
    let id = canopy
        .core
        .input_map
        .bindings()
        .iter()
        .find(|record| record.tier == crate::BindingTier::Mode("prefix".to_string()))
        .expect("mode binding")
        .id;

    let bound = canopy.explain_key(None, 'y'.into())?;
    assert_eq!(
        bound.outcome,
        RouteOutcome::Transient(RouteWinner {
            binding: id,
            node: canopy.root_id(),
            path: Path::from("/root"),
            kind: crate::BindingActionKind::Script,
            phase: crate::BindingPhase::AfterWidget,
        })
    );
    assert!(bound.steps.is_empty());
    assert!(KeyExpectation::Transient(id).matches(&bound.outcome));
    assert!(!KeyExpectation::TransientDismiss.matches(&bound.outcome));

    let dismissed = canopy.explain_key(None, 'z'.into())?;
    assert_eq!(dismissed.outcome, RouteOutcome::TransientDismiss);
    assert!(KeyExpectation::TransientDismiss.matches(&dismissed.outcome));
    assert!(!KeyExpectation::Transient(id).matches(&dismissed.outcome));
    Ok(())
}

#[test]
fn send_key_checked_rechecks_between_calls() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(
        r#"canopy.bind("x", { description = "Switch" }, function() canopy.set_mode("ran") end)"#,
    )?;
    let id = canopy
        .core
        .input_map
        .bindings()
        .iter()
        .find(|record| record.input == InputSpec::Key('x'.into()))
        .expect("binding")
        .id;
    canopy.send_key_checked('x', KeyExpectation::Binding(id))?;
    assert_eq!(canopy.mode(), "ran");

    // A later evaluation that removes the winner makes the next check fail.
    canopy.eval_script(&format!("canopy.unbind({})", id.as_u64()))?;
    let error = canopy
        .send_key_checked('x', KeyExpectation::Binding(id))
        .expect_err("a stale expectation must be rejected");
    assert!(matches!(error, Error::KeyDispatchDivergence(_)));
    Ok(())
}

#[test]
fn send_key_checked_rechecks_focus_and_tree_changes() -> Result<()> {
    let mut canopy = app();
    let first = focused_leaf(&mut canopy, PredictingLeaf)?;
    let second = canopy.core.create_detached(PredictingLeaf)?;
    canopy.core.attach(canopy.root_id(), second)?;

    // Focus moving to another consumer invalidates the first expectation.
    canopy.core.set_focus(second)?;
    let error = canopy
        .send_key_checked('x', KeyExpectation::Widget(first))
        .expect_err("stale focus must be rejected");
    assert!(matches!(error, Error::KeyDispatchDivergence(_)));

    // A removed node can no longer be the expected consumer.
    canopy.core.set_focus(first)?;
    canopy.core.remove_subtree(second)?;
    let error = canopy
        .send_key_checked('x', KeyExpectation::Widget(second))
        .expect_err("a removed node must be rejected");
    assert!(matches!(error, Error::KeyDispatchDivergence(_)));
    Ok(())
}

#[test]
fn send_key_checked_rejects_a_mismatched_expectation_without_delivery() -> Result<()> {
    let mut canopy = app();
    canopy.eval_script(
        r#"canopy.bind("x", { description = "Switch" }, function() canopy.set_mode("ran") end)"#,
    )?;
    let error = canopy
        .send_key_checked('x', KeyExpectation::Unhandled)
        .expect_err("expectation must be rejected");
    assert!(matches!(error, Error::KeyDispatchDivergence(_)));
    assert_eq!(canopy.mode(), "");
    assert!(
        !canopy
            .route_trace()
            .iter()
            .any(|entry| entry.kind == RouteTraceKind::RunBinding)
    );
    Ok(())
}

/// A leaf that accepts one intent and records what it receives.
struct IntentLeaf {
    /// Intent name this leaf implements.
    intent: &'static str,
    /// Whether the pure prediction accepts the intent.
    accepted: bool,
    /// Whether `on_intent` honors an accepted intent.
    honest: bool,
    /// Number of completed intent calls.
    calls: Arc<AtomicUsize>,
    /// Number of raw key events the leaf observed.
    raw: Arc<AtomicUsize>,
}

impl Widget for IntentLeaf {
    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(_)) {
            self.raw.fetch_add(1, Ordering::Relaxed);
        }
        Ok(EventOutcome::Ignore)
    }

    fn accepts_intent(&self, intent: &str, _context: &dyn ViewContext) -> bool {
        self.accepted && intent == self.intent
    }

    fn on_intent(&mut self, intent: &str, _context: &mut dyn Context) -> Result<EventOutcome> {
        if !self.accepted || !self.honest || intent != self.intent {
            return Ok(EventOutcome::Ignore);
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(EventOutcome::Handle)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("intent_leaf")
    }
}

/// Mount one action leaf and give it the keyboard.
fn mount_action_leaf(canopy: &mut Canopy, leaf: IntentLeaf) -> Result<NodeId> {
    let node = canopy.core.create_detached(leaf)?;
    canopy.core.attach(canopy.core.root, node)?;
    canopy.core.set_focus(node)?;
    Ok(node)
}

#[test]
fn the_rendered_api_narrows_the_action_arm_to_registered_names() -> Result<()> {
    let empty = app();
    assert!(
        !empty.script_api()?.contains("IntentName"),
        "an empty catalog leaves the action arm at commands and callbacks"
    );

    let mut canopy = app_with(|setup| {
        setup.register_intent(inputmap::IntentSpec::new(
            "test.clear",
            "Clear the test leaf",
        )?)
    });
    let api = canopy.script_api()?;
    assert!(
        api.contains("export type IntentName = \"test.clear\""),
        "the union holds every registered name: {api}"
    );
    assert!(
        api.contains("`test.clear`: Clear the test leaf"),
        "the union documents each action: {api}"
    );
    let unknown = canopy.check_script(
        "unknown-action",
        "--!strict\ncanopy.bind(\"x\", {description = \"x\"}, \"test.other\")",
    )?;
    assert!(
        unknown.has_errors(),
        "an unregistered action name fails the typechecker: {unknown:?}"
    );
    let known = canopy.check_script(
        "known-action",
        "--!strict\ncanopy.bind(\"x\", {description = \"x\"}, \"test.clear\")",
    )?;
    assert!(
        !known.has_errors(),
        "a registered action name typechecks: {known:?}"
    );
    Ok(())
}

#[test]
fn an_intent_dispatches_to_its_accepting_consumer() -> Result<()> {
    let mut canopy = clear_intent_app();
    let calls = Arc::new(AtomicUsize::new(0));
    let leaf = mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    canopy.eval_script(r#"canopy.bind("ctrl-x", {description = "Clear"}, "test.clear")"#)?;
    let key = key::Key::parse_spec("ctrl-x")?;
    canopy.key(None, key)?;
    assert_eq!(calls.load(Ordering::Relaxed), 1, "the consumer runs once");

    let explanation = canopy.core.explain_key(Some(leaf), key)?;
    assert!(
        matches!(
            explanation.outcome,
            RouteOutcome::Binding(RouteWinner {
                kind: crate::BindingActionKind::Intent,
                phase: crate::BindingPhase::BeforeWidget,
                ..
            })
        ),
        "the analysis names the action, got {:?}",
        explanation.outcome
    );
    let snapshot = canopy.available_bindings(Some(leaf))?;
    assert!(
        snapshot.bindings.iter().any(|binding| {
            binding.description == "Clear"
                && binding.action == inputmap::BindingActionKind::Intent
                && binding
                    .intent
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "test.clear")
        }),
        "help shows the exact intent row"
    );
    Ok(())
}

#[test]
fn a_dormant_action_does_not_shadow_the_next_candidate() -> Result<()> {
    let mut canopy = clear_intent_app();
    let raw = Arc::new(AtomicUsize::new(0));
    let leaf = mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: false,
            honest: true,
            calls: Arc::new(AtomicUsize::new(0)),
            raw: Arc::clone(&raw),
        },
    )?;
    canopy.eval_script(
        r#"
        canopy.bind("ctrl-x", {description = "Clear", path = "intent_leaf/"}, "test.clear")
        canopy.bind("ctrl-x", {description = "Fallback"}, function()
            canopy.set_mode("fallback")
        end)
        "#,
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    canopy.key(None, key)?;
    assert_eq!(canopy.mode(), "fallback", "the fallback runs");
    assert_eq!(
        raw.load(Ordering::Relaxed),
        1,
        "the raw key reaches the widget first"
    );
    let explanation = canopy.core.explain_key(Some(leaf), key)?;
    assert!(matches!(
        explanation.outcome,
        RouteOutcome::Binding(RouteWinner {
            phase: crate::BindingPhase::AfterWidget,
            ..
        })
    ));
    Ok(())
}

#[test]
fn a_dormant_global_action_falls_through_to_the_default_tier() -> Result<()> {
    let mut canopy = clear_intent_app();
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: false,
            honest: true,
            calls: Arc::new(AtomicUsize::new(0)),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    canopy.eval_script(
        r#"
        canopy.bind("ctrl-x", {
            description = "Global clear",
            tier = "global",
            path = "/root/**/",
        }, "test.clear")
        canopy.bind("ctrl-x", {description = "Default fallback"}, function()
            canopy.set_mode("default_won")
        end)
        "#,
    )?;
    canopy.key(None, key::Key::parse_spec("ctrl-x")?)?;
    assert_eq!(
        canopy.mode(),
        "default_won",
        "the dormant global action gives way to the default tier"
    );
    Ok(())
}

#[test]
fn a_transient_mode_offers_its_action_before_the_raw_key() -> Result<()> {
    let mut canopy = clear_intent_app();
    let calls = Arc::new(AtomicUsize::new(0));
    let raw = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::clone(&raw),
        },
    )?;
    canopy.eval_script(
        r#"
        canopy.keymap({
            mode = "prefix",
            { key = "ctrl-x", description = "Clear", action = "test.clear" },
        })
        canopy.push_mode("prefix", {transient = true})
        "#,
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    let explanation = canopy.core.explain_key(None, key)?;
    assert!(
        matches!(
            explanation.outcome,
            RouteOutcome::Transient(RouteWinner {
                kind: crate::BindingActionKind::Intent,
                ..
            })
        ),
        "the transient analysis names the action, got {:?}",
        explanation.outcome
    );
    canopy.key(None, key)?;
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(raw.load(Ordering::Relaxed), 0, "no raw key is delivered");
    assert_eq!(canopy.mode(), "", "the transient mode pops");
    Ok(())
}

#[test]
fn a_transient_mode_dismisses_a_dormant_action() -> Result<()> {
    let mut canopy = clear_intent_app();
    let raw = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: false,
            honest: true,
            calls: Arc::new(AtomicUsize::new(0)),
            raw: Arc::clone(&raw),
        },
    )?;
    canopy.eval_script(
        r#"
        canopy.keymap({
            mode = "prefix",
            { key = "ctrl-x", description = "Clear", action = "test.clear" },
        })
        canopy.bind("ctrl-x", {description = "Default"}, function()
            canopy.set_mode("default_won")
        end)
        canopy.push_mode("prefix", {transient = true})
        "#,
    )?;
    canopy.key(None, key::Key::parse_spec("ctrl-x")?)?;
    assert_eq!(canopy.mode(), "", "the transient mode dismisses");
    assert_eq!(
        raw.load(Ordering::Relaxed),
        0,
        "the raw key is not delivered"
    );
    Ok(())
}

#[test]
fn only_the_first_accepting_consumer_runs() -> Result<()> {
    let mut canopy = clear_intent_app();
    let leaf_calls = Arc::new(AtomicUsize::new(0));
    let parent_calls = Arc::new(AtomicUsize::new(0));
    let parent = canopy.core.create_detached(IntentLeaf {
        intent: "test.clear",
        accepted: true,
        honest: true,
        calls: Arc::clone(&parent_calls),
        raw: Arc::new(AtomicUsize::new(0)),
    })?;
    canopy.core.attach(canopy.core.root, parent)?;
    let leaf = canopy.core.create_detached(IntentLeaf {
        intent: "test.clear",
        accepted: true,
        honest: true,
        calls: Arc::clone(&leaf_calls),
        raw: Arc::new(AtomicUsize::new(0)),
    })?;
    canopy.core.attach(parent, leaf)?;
    canopy.core.set_focus(leaf)?;
    canopy.eval_script(r#"canopy.bind("ctrl-x", {description = "Clear"}, "test.clear")"#)?;
    canopy.key(None, key::Key::parse_spec("ctrl-x")?)?;
    assert_eq!(leaf_calls.load(Ordering::Relaxed), 1, "the child runs");
    assert_eq!(
        parent_calls.load(Ordering::Relaxed),
        0,
        "the ancestor does not"
    );
    Ok(())
}

#[test]
fn a_declining_child_leaves_an_action_to_its_accepting_ancestor() -> Result<()> {
    let mut canopy = clear_intent_app();
    let parent_calls = Arc::new(AtomicUsize::new(0));
    let parent = canopy.core.create_detached(IntentLeaf {
        intent: "test.clear",
        accepted: true,
        honest: true,
        calls: Arc::clone(&parent_calls),
        raw: Arc::new(AtomicUsize::new(0)),
    })?;
    canopy.core.attach(canopy.core.root, parent)?;
    let leaf = canopy.core.create_detached(IntentLeaf {
        intent: "test.clear",
        accepted: false,
        honest: true,
        calls: Arc::new(AtomicUsize::new(0)),
        raw: Arc::new(AtomicUsize::new(0)),
    })?;
    canopy.core.attach(parent, leaf)?;
    canopy.core.set_focus(leaf)?;
    let key = key::Key::parse_spec("ctrl-x")?;
    let id = bind_intent(
        &mut canopy,
        key,
        default_options(None, "Clear", None)?,
        inputmap::IntentName::new("test.clear")?,
    )?;
    canopy.key(None, key)?;
    assert_eq!(
        parent_calls.load(Ordering::Relaxed),
        1,
        "the ancestor consumes the action the child declined"
    );
    assert_ne!(
        canopy.core.binding_verdict(id, leaf),
        BindingVerdict::NoConsumer,
        "the diagnostic follows the same route as dispatch"
    );
    Ok(())
}

/// A leaf with one command that is always disabled.
struct DisabledLeaf;

#[derive_commands]
impl DisabledLeaf {
    fn ready(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(CommandStatus::Disabled("not ready".into()))
    }

    #[command(enabled = "ready")]
    fn fire(&self) {}
}

impl Widget for DisabledLeaf {
    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("disabled_leaf")
    }
}

/// Build binding options for an action or command in the default tier.
fn default_options(
    path: Option<&str>,
    description: &str,
    phase: Option<inputmap::BindingPhase>,
) -> Result<inputmap::BindingOptions> {
    Ok(inputmap::BindingOptions {
        path: path.map(str::parse).transpose()?,
        tier: inputmap::BindingTier::Default,
        description: description.to_string(),
        source: None,
        phase,
    })
}

#[test]
fn a_disabled_command_claims_the_key_after_a_dormant_action() -> Result<()> {
    let mut canopy = clear_intent_app();
    canopy.core.commands.add(DisabledLeaf::commands())?;
    let calls = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: false,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    bind_intent(
        &mut canopy,
        key,
        default_options(Some("intent_leaf/"), "Dormant clear", None)?,
        inputmap::IntentName::new("test.clear")?,
    )?;
    bind_command(
        &mut canopy,
        key,
        default_options(
            None,
            "Disabled fire",
            Some(inputmap::BindingPhase::AfterWidget),
        )?,
        DisabledLeaf::call_fire(),
    )?;
    canopy.key(None, key)?;
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| entry.detail.contains("binding disabled")),
        "the dormant intent gives way to the disabled command: {:?}",
        canopy.route_trace()
    );
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "the dormant intent never runs"
    );
    Ok(())
}

#[test]
fn a_disabled_command_claims_the_key_before_an_accepting_intent() -> Result<()> {
    let mut canopy = clear_intent_app();
    canopy.core.commands.add(DisabledLeaf::commands())?;
    let calls = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    bind_command(
        &mut canopy,
        key,
        default_options(
            Some("intent_leaf/"),
            "Disabled fire",
            Some(inputmap::BindingPhase::AfterWidget),
        )?,
        DisabledLeaf::call_fire(),
    )?;
    bind_intent(
        &mut canopy,
        key,
        default_options(None, "Clear", None)?,
        inputmap::IntentName::new("test.clear")?,
    )?;
    canopy.key(None, key)?;
    assert_eq!(
        calls.load(Ordering::Relaxed),
        0,
        "the higher-ranked disabled command hides the intent"
    );
    assert!(
        canopy
            .route_trace()
            .iter()
            .any(|entry| entry.detail.contains("binding disabled")),
        "the disabled command claims the key"
    );
    Ok(())
}

#[test]
fn an_unreadable_widget_declines_an_action() -> Result<()> {
    let mut canopy = clear_intent_app();
    let calls = Arc::new(AtomicUsize::new(0));
    let leaf = mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    bind_intent(
        &mut canopy,
        key,
        default_options(None, "Clear", None)?,
        inputmap::IntentName::new("test.clear")?,
    )?;
    let path = canopy.core.path_of(canopy.core.root, leaf);
    canopy
        .core
        .with_widget_dyn_mut(leaf, |_, core| -> Result<()> {
            assert!(
                core.select_key_binding(leaf, &path, key, leaf, &[])
                    .is_none(),
                "a borrowed widget declines the action"
            );
            let explanation = core.explain_key(Some(leaf), key)?;
            assert!(
                matches!(explanation.outcome, RouteOutcome::Unhandled),
                "an unreadable consumer is not an action consumer, got {:?}",
                explanation.outcome
            );
            Ok(())
        })??;
    canopy.key(None, key)?;
    assert_eq!(calls.load(Ordering::Relaxed), 1, "the action works again");
    Ok(())
}

#[test]
fn checked_dispatch_accepts_an_action_expectation() -> Result<()> {
    let mut canopy = clear_intent_app();
    let calls = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: true,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    let key = key::Key::parse_spec("ctrl-x")?;
    let id = bind_intent(
        &mut canopy,
        key,
        default_options(None, "Clear", None)?,
        inputmap::IntentName::new("test.clear")?,
    )?;
    canopy.send_key_checked(key, KeyExpectation::Binding(id))?;
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    let error = canopy
        .send_key_checked(key, KeyExpectation::Unhandled)
        .expect_err("a mismatched expectation is rejected");
    assert!(matches!(error, Error::KeyDispatchDivergence(_)));
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "a rejected expectation delivers nothing"
    );
    Ok(())
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "accepts_intent promised Handle")]
fn an_inconsistent_action_consumer_trips_the_debug_assert() {
    let mut canopy = clear_intent_app();
    let calls = Arc::new(AtomicUsize::new(0));
    mount_action_leaf(
        &mut canopy,
        IntentLeaf {
            intent: "test.clear",
            accepted: true,
            honest: false,
            calls: Arc::clone(&calls),
            raw: Arc::new(AtomicUsize::new(0)),
        },
    )
    .unwrap();
    canopy
        .eval_script(r#"canopy.bind("ctrl-x", {description = "Clear"}, "test.clear")"#)
        .unwrap();
    drop(canopy.key(None, key::Key::parse_spec("ctrl-x").unwrap()));
}
