//! Cleanup regressions for synchronous adapters and completed VM wake signals.

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Context as TaskContext,
    time::Duration,
};

use futures::{channel::mpsc, task::noop_waker};
use tokio::{runtime::Builder, time::sleep};

use super::{
    AdapterEvent, Canopy, CanopyBuilder, EvalRequest, EvalTicket, TurnInput, scripting::Startup,
};
use crate::{
    Context, ViewContext,
    commands::ArgValue,
    derive_commands,
    error::{Error, Result},
    geom::Size,
    input::Event,
    layout::Layout,
    render::Render,
    runtime::{NodeWakeHandle, PollLifetime},
    widget::{EventOutcome, Widget},
};

/// Input fixture that makes its mutation observable even when dispatch fails.
struct CleanupProbe {
    /// Queue used by the script predicate to request a failing input event.
    events: mpsc::UnboundedSender<AdapterEvent>,
    /// Mutation counter that must survive adapter cleanup.
    mutations: Arc<AtomicUsize>,
}

#[derive_commands]
impl CleanupProbe {
    /// Queue input only after the script reaches its wait predicate.
    #[command]
    fn queue_failure(&self) -> Result<()> {
        self.events
            .unbounded_send(AdapterEvent::Input(Event::Key('x'.into())))
            .map_err(|error| Error::Driver(error.to_string()))
    }
}

impl Widget for CleanupProbe {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(_)) {
            self.mutations.fetch_add(1, Ordering::Relaxed);
            return Err(Error::Invalid("input failed after mutation".into()));
        }
        Ok(EventOutcome::Ignore)
    }
}

/// Construct a rendered application with the input probe focused.
fn application() -> Result<(Canopy, Arc<AtomicUsize>)> {
    let mut canopy = CanopyBuilder::new()
        .configure(|setup| setup.add_commands::<CleanupProbe>())
        .build()?;
    let mutations = Arc::new(AtomicUsize::new(0));
    canopy.replace_root(CleanupProbe {
        events: canopy.event_tx.clone(),
        mutations: Arc::clone(&mutations),
    })?;
    canopy.set_screen_size(Size::new(10, 3))?;
    canopy.core.set_focus(canopy.root_id())?;
    canopy.turn(TurnInput::Prepare)?;
    Ok((canopy, mutations))
}

#[test]
fn completed_script_does_not_leave_a_ready_runtime_wake() -> Result<()> {
    let (mut canopy, _) = application()?;
    let mut outcome = canopy.turn(TurnInput::StartEval(EvalRequest {
        source: "canopy.wait_for(function() return true end); return 42".into(),
        timeout: None,
    }))?;
    for _ in 0..20 {
        if !outcome.completed.is_empty() {
            break;
        }
        outcome = canopy.turn(TurnInput::Wake)?;
    }
    assert_eq!(outcome.completed.len(), 1);
    assert!(matches!(
        outcome.completed[0].result.as_ref(),
        Ok(ArgValue::Int(42))
    ));
    let waker = noop_waker();
    let context = TaskContext::from_waker(&waker);
    assert!(canopy.poll_runtime_wake(&context).is_pending());
    canopy.turn(TurnInput::Wake)?;
    assert!(canopy.poll_runtime_wake(&context).is_pending());
    Ok(())
}

#[test]
fn headless_input_failure_releases_parked_eval_and_preserves_mutation() -> Result<()> {
    let (mut canopy, mutations) = application()?;
    let source = "canopy.wait_for(function() cleanup_probe.queue_failure(); return false end)";
    let error = canopy
        .eval_script(source)
        .expect_err("queued input fails while evaluation waits");
    assert!(error.to_string().contains("input failed after mutation"));
    assert_eq!(mutations.load(Ordering::Relaxed), 1);
    assert!(!canopy.script.host.is_eval_active());
    let entries: Vec<_> = canopy
        .script_journal()
        .iter()
        .filter(|entry| entry.source == source)
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].ok);
    let waker = noop_waker();
    assert!(
        canopy
            .poll_runtime_wake(&TaskContext::from_waker(&waker))
            .is_pending()
    );
    assert_eq!(canopy.eval_script("return 7")?, ArgValue::Int(7));
    assert_eq!(mutations.load(Ordering::Relaxed), 1);
    assert_eq!(
        canopy
            .script_journal()
            .iter()
            .filter(|entry| entry.source == source)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn cancellable_headless_waits_and_reuse_work_in_supported_contexts() -> Result<()> {
    let exercise = || -> Result<()> {
        let (mut canopy, _) = application()?;
        let request = |source: &str| EvalRequest {
            source: source.into(),
            timeout: Some(Duration::from_secs(2)),
        };
        let ready =
            request("for _ = 1, 300 do canopy.wait_for(function() return true end) end return 42");
        let parked = request("canopy.wait_for(function() return false end, 20)");
        let cancelled = [
            request("canopy.wait_for(function() return false end)"),
            request("while true do end"),
        ];
        assert_eq!(canopy.eval(ready)?.into_result()?, ArgValue::Int(42));
        let outcome = canopy.eval(parked)?;
        assert!(matches!(
            outcome.result,
            Err(Error::ScriptTimeout { timeout_ms: 20 })
        ));
        for request in cancelled {
            assert!(matches!(
                canopy.eval_with_cancellation(request, async {
                    sleep(Duration::from_millis(20)).await;
                }),
                Err(Error::ScriptCancelled)
            ));
            assert!(!canopy.script.host.is_eval_active());
            assert!(canopy.event_rx.is_some());
            assert!(
                canopy
                    .script_journal()
                    .last()
                    .expect("cancelled journal entry")
                    .error
                    .as_ref()
                    .expect("cancellation recorded")
                    .contains("cancel")
            );
            assert_eq!(
                canopy
                    .eval(EvalRequest {
                        source: "return 7".into(),
                        timeout: None,
                    })?
                    .into_result()?,
                ArgValue::Int(7)
            );
        }
        Ok(())
    };
    exercise()?;
    // Terminal adapters enter a current-thread runtime without polling turns
    // from one of its tasks.
    let runtime = Builder::new_current_thread().enable_all().build().unwrap();
    {
        let _entered = runtime.enter();
        exercise()?;
    }
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { exercise() })?;
    Ok(())
}

#[test]
fn synchronous_headless_rejects_current_thread_tasks_without_consuming_driver() -> Result<()> {
    let mut canopy = CanopyBuilder::new()
        .configure(|setup| {
            setup.register_startup_script("startup", "function setup() print('startup ran') end")
        })
        .build()?;
    Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime")
        .block_on(async {
            let error = canopy
                .eval_script("return 1")
                .expect_err("cannot block this task");
            assert!(matches!(error, Error::Invalid(_)));
            assert!(error.to_string().contains("blocking worker"));
            assert!(canopy.event_rx.is_some());
            assert_eq!(canopy.driver.startup, Startup::Pending);
            Ok::<_, Error>(())
        })?;
    assert_eq!(canopy.eval_script("return 7")?, ArgValue::Int(7));
    assert_eq!(canopy.driver.startup, Startup::Ran);
    Ok(())
}

#[test]
fn dropping_live_ticket_cancels_before_admission_and_while_parked() -> Result<()> {
    for admitted in [false, true] {
        let (mut canopy, _) = application()?;
        let source = "canopy.wait_for(function() return false end)";
        let ticket = canopy.automation_handle().submit_eval(EvalRequest {
            source: source.into(),
            timeout: None,
        })?;
        if admitted {
            canopy.turn(TurnInput::Wake)?;
            assert!(canopy.script.host.is_eval_active());
            // Drain the initial VM notification so the dropped ticket must
            // supply its own wake.
            for _ in 0..10 {
                canopy.turn(TurnInput::Wake)?;
            }
            let waker = noop_waker();
            assert!(
                canopy
                    .poll_runtime_wake(&TaskContext::from_waker(&waker))
                    .is_pending()
            );
        }
        drop(ticket);
        if admitted {
            let waker = noop_waker();
            assert!(
                canopy
                    .poll_runtime_wake(&TaskContext::from_waker(&waker))
                    .is_ready()
            );
        }
        canopy.turn(TurnInput::Wake)?;
        assert!(!canopy.script.host.is_eval_active());
        assert_eq!(
            canopy
                .script_journal()
                .iter()
                .filter(|entry| entry.source == source)
                .count(),
            usize::from(admitted)
        );
        assert_eq!(canopy.eval_script("return 7")?, ArgValue::Int(7));
    }
    Ok(())
}

/// Root widget whose poll fails fatally on demand.
#[derive(Clone, Default)]
struct FailingPoll {
    /// Wake handle captured on mount.
    wake: Rc<RefCell<Option<NodeWakeHandle>>>,
    /// Whether the next poll fails with a fatal error.
    fail: Rc<Cell<bool>>,
}

impl Widget for FailingPoll {
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        *self.wake.borrow_mut() = Some(ctx.wake_handle(PollLifetime::Node)?);
        Ok(())
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
        if self.fail.get() {
            return Err(Error::Internal("poll broke".into()));
        }
        Ok(None)
    }
}

/// Construct a rendered application whose root poll can fail, with a parked
/// evaluation submitted through an automation ticket.
fn parked_ticket(source: &str) -> Result<(Canopy, FailingPoll, EvalTicket)> {
    let probe = FailingPoll::default();
    let mut canopy = CanopyBuilder::new().build()?;
    canopy.replace_root(probe.clone())?;
    canopy.set_screen_size(Size::new(10, 3))?;
    canopy.turn(TurnInput::Prepare)?;
    let ticket = canopy
        .automation_handle()
        .submit_eval(EvalRequest::new(source))?;
    for _ in 0..10 {
        canopy.turn(TurnInput::Wake)?;
    }
    assert!(canopy.script.host.is_eval_active());
    assert!(canopy.driver.publication.waiter_count() > 0);
    Ok((canopy, probe, ticket))
}

/// Assert that a turn's failure settled the ticket with `expected`, journaled
/// the evaluation, released its waiters, and left the driver admitting work.
fn assert_settled(
    canopy: &mut Canopy,
    mut ticket: EvalTicket,
    source: &str,
    expected: &str,
) -> Result<()> {
    let outcome = ticket
        .completion
        .try_recv()
        .expect("the ticket is live")
        .expect("the ticket settles in the failing turn");
    assert_eq!(outcome.id, ticket.id);
    let error = outcome
        .result
        .expect_err("the evaluation reports the failure");
    assert!(error.to_string().contains(expected), "{error}");
    let entries: Vec<_> = canopy
        .script_journal()
        .iter()
        .filter(|entry| entry.source == source)
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].ok);
    assert_eq!(canopy.driver.publication.waiter_count(), 0);
    assert!(!canopy.script.host.is_eval_active());
    assert_eq!(canopy.eval_script("return 7")?, ArgValue::Int(7));
    Ok(())
}

#[test]
fn regression_fatal_poll_in_a_cancel_turn_settles_the_ticket() -> Result<()> {
    let source = "canopy.wait_for(function() return false end)";
    let (mut canopy, probe, ticket) = parked_ticket(source)?;
    probe.fail.set(true);
    probe.wake.borrow().as_ref().expect("mounted").wake()?;
    let Err(error) = canopy.turn(TurnInput::CancelEval(ticket.id)) else {
        panic!("the poll fails the turn");
    };
    assert!(error.to_string().contains("poll broke"), "{error}");
    probe.fail.set(false);
    assert_settled(&mut canopy, ticket, source, "poll broke")
}

#[test]
fn regression_failed_abort_settles_the_ticket() -> Result<()> {
    let source = "canopy.wait_for(function() return false end)";
    let (mut canopy, _probe, ticket) = parked_ticket(source)?;
    let host = canopy.script.host.clone();
    let held = host.hold_runtime_for_testing();
    let Err(error) = canopy.turn(TurnInput::CancelEval(ticket.id)) else {
        panic!("the abort fails the turn");
    };
    drop(held);
    assert!(error.to_string().contains("abort"), "{error}");
    assert_settled(&mut canopy, ticket, source, "abort")
}

/// Root widget whose key handler panics while its flag is set.
#[derive(Clone, Default)]
struct PanicKey {
    /// Whether the next key panics.
    panics: Rc<Cell<bool>>,
    /// Count of keys handled.
    keys: Rc<Cell<usize>>,
}

impl Widget for PanicKey {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(_)) {
            self.keys.set(self.keys.get() + 1);
            assert!(!self.panics.replace(false), "key panic");
        }
        Ok(EventOutcome::Ignore)
    }
}

#[test]
fn regression_a_turn_runs_after_a_caught_panic_in_a_turn() -> Result<()> {
    let probe = PanicKey::default();
    let mut canopy = CanopyBuilder::new().build()?;
    canopy.replace_root(probe.clone())?;
    canopy.set_screen_size(Size::new(10, 3))?;
    canopy.core.set_focus(canopy.root_id())?;
    canopy.turn(TurnInput::Prepare)?;
    probe.panics.set(true);
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let _ignored = canopy.turn(TurnInput::Events(vec![Event::Key('x'.into())]));
    }));
    assert!(caught.is_err());
    canopy.turn(TurnInput::Events(vec![Event::Key('y'.into())]))?;
    assert_eq!(probe.keys.get(), 2);
    canopy.prepare()?;
    Ok(())
}

#[test]
fn eval_admission_reports_each_busy_cause() -> Result<()> {
    let (mut canopy, _) = application()?;
    canopy.eval_admission()?;

    // The driver holds a parked evaluation.
    let started = canopy.turn(TurnInput::StartEval(EvalRequest::new(
        "canopy.wait_for(function() return false end)",
    )))?;
    assert!(matches!(canopy.eval_admission(), Err(Error::ScriptBusy(_))));
    let id = started.started.expect("the evaluation starts");
    canopy.turn(TurnInput::CancelEval(id))?;
    canopy.eval_admission()?;

    // The host holds an invocation it started itself.
    let host = canopy.script.host.clone();
    let script = host.compile("return 1")?;
    let root = canopy.root_id();
    let invocation = host.start_invocation(root, canopy.core.nodes[root].incarnation, script)?;
    assert!(matches!(canopy.eval_admission(), Err(Error::ScriptBusy(_))));
    drop(invocation);
    canopy.eval_admission()?;

    // A script runs in a live VM scope.
    canopy.script.context_stack.push(root);
    assert!(matches!(canopy.eval_admission(), Err(Error::ScriptBusy(_))));
    canopy.script.context_stack.clear();
    canopy.eval_admission()
}

/// Root widget that requests an exit and then fails, in its key handler, its
/// poll, or its render.
#[derive(Clone, Default)]
struct ExitThenFail {
    /// Wake handle captured on mount.
    wake: Rc<RefCell<Option<NodeWakeHandle>>>,
    /// Where the next failure happens: 1 key, 2 poll, 3 render.
    stage: Rc<Cell<u8>>,
    /// Count of keys seen.
    keys: Rc<Cell<usize>>,
}

impl Widget for ExitThenFail {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        *self.wake.borrow_mut() = Some(ctx.wake_handle(PollLifetime::Node)?);
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(_)) {
            self.keys.set(self.keys.get() + 1);
            if self.stage.get() == 1 {
                self.stage.set(0);
                ctx.exit(3);
                return Err(Error::Invalid("key failed".into()));
            }
            if self.stage.get() == 3 {
                ctx.exit(3);
            }
        }
        Ok(EventOutcome::Ignore)
    }

    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        if self.stage.get() == 2 {
            self.stage.set(0);
            ctx.exit(3);
            return Err(Error::Internal("poll failed".into()));
        }
        Ok(None)
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        if self.stage.get() == 3 {
            self.stage.set(0);
            return Err(Error::Internal("render failed".into()));
        }
        Ok(())
    }
}

#[test]
fn regression_a_failed_turn_carries_its_exit() -> Result<()> {
    let probe = ExitThenFail::default();
    let mut canopy = CanopyBuilder::new().build()?;
    canopy.replace_root(probe.clone())?;
    canopy.set_screen_size(Size::new(10, 3))?;
    canopy.core.set_focus(canopy.root_id())?;
    canopy.turn(TurnInput::Prepare)?;
    let keys = || TurnInput::Events(vec![Event::Key('a'.into()), Event::Key('b'.into())]);

    for (stage, work, message) in [
        (1, keys(), "key failed"),
        (2, TurnInput::Wake, "poll failed"),
        (
            3,
            TurnInput::Events(vec![Event::Key('x'.into())]),
            "render failed",
        ),
    ] {
        probe.stage.set(stage);
        if stage == 2 {
            probe.wake.borrow().as_ref().expect("mounted").wake()?;
        }
        let Err(failure) = canopy.turn(work) else {
            panic!("stage {stage} fails its turn");
        };
        assert!(failure.error.to_string().contains(message), "{failure}");
        assert_eq!(failure.exit_code, Some(3), "stage {stage} carries its exit");
        let before = probe.keys.get();
        let outcome = canopy.turn(keys())?;
        assert_eq!(
            probe.keys.get(),
            before + 2,
            "the next turn takes its whole batch"
        );
        assert_eq!(outcome.exit_code, None);
    }
    Ok(())
}
