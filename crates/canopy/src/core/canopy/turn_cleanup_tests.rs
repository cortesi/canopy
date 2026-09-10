//! Cleanup regressions for synchronous adapters and completed VM wake signals.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Context as TaskContext,
};

use futures::{channel::mpsc::UnboundedSender, task::noop_waker};

use super::{AdapterEvent, Canopy, EvalRequest, Work};
use crate::{
    Context, ViewContext,
    commands::ArgValue,
    derive_commands,
    error::{Error, Result},
    event::Event,
    geom::Size,
    widget::{EventOutcome, Widget},
};

/// Input fixture that makes its mutation observable even when dispatch fails.
struct CleanupProbe {
    /// Queue used by the script predicate to request a failing input event.
    events: UnboundedSender<AdapterEvent>,
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
            .map_err(|error| Error::RunLoop(error.to_string()))
    }
}

impl Widget for CleanupProbe {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Key(_)) {
            self.mutations.fetch_add(1, Ordering::Relaxed);
            return Err(Error::InvalidOperation(
                "input failed after mutation".into(),
            ));
        }
        Ok(EventOutcome::Ignore)
    }
}

/// Construct a rendered application with the input probe focused.
fn application() -> Result<(Canopy, Arc<AtomicUsize>)> {
    let mut canopy = Canopy::new();
    canopy.add_commands::<CleanupProbe>()?;
    canopy.finalize_api()?;
    let mutations = Arc::new(AtomicUsize::new(0));
    canopy.replace_root(CleanupProbe {
        events: canopy.event_tx.clone(),
        mutations: Arc::clone(&mutations),
    })?;
    canopy.set_root_size(Size::new(10, 3))?;
    canopy.core.set_focus(canopy.root_id())?;
    canopy.turn(Work::Prepare)?;
    Ok((canopy, mutations))
}

#[test]
fn completed_script_does_not_leave_a_ready_runtime_wake() -> Result<()> {
    let (mut canopy, _) = application()?;
    let mut outcome = canopy.turn(Work::StartEval(EvalRequest {
        source: "canopy.wait_for(function() return true end); return 42".into(),
        timeout: None,
        anchor: canopy.root_id(),
    }))?;
    for _ in 0..20 {
        if !outcome.completed.is_empty() {
            break;
        }
        outcome = canopy.turn(Work::Wake)?;
    }
    assert_eq!(outcome.completed.len(), 1);
    assert!(matches!(
        outcome.completed[0].result.as_ref(),
        Ok(ArgValue::Int(42))
    ));
    let waker = noop_waker();
    let context = TaskContext::from_waker(&waker);
    assert!(canopy.poll_runtime_wake(&context).is_pending());
    canopy.turn(Work::Wake)?;
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
    assert!(!canopy.script_host.is_eval_active());
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
