//! Crossterm terminal adapter: run options, interrupt policy, and the run
//! loop.

mod input;
mod output;
mod session;

use std::io;

use crossterm::{event as cevent, terminal};
use tokio::runtime::Builder;

use self::input::EventSource;
pub use self::{output::CrosstermRender, session::CrosstermControl};
use crate::{
    Canopy, Work,
    backend::TerminalSession,
    core::{Core, canopy::WorkSelector, dump::dump},
    error::{self, Result},
    event::{Event, key},
    geom::Size,
};

/// Host handling of terminal Ctrl+C input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterruptPolicy {
    /// Restore the terminal and exit the host loop with status 130.
    #[default]
    Exit130,
    /// Deliver Ctrl+C through normal application input routing.
    RouteToApplication,
}

/// Terminal adapter policy, applied before application input dispatch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Policy for Ctrl+C.
    pub interrupt_policy: InterruptPolicy,
    /// Optional exact key that exits even when Ctrl+C is routed.
    pub emergency_exit: Option<key::Key>,
}

/// Decide host interruption without starting or mutating a terminal session.
///
/// An interrupt anywhere in a batch stops the host before the batch dispatches.
fn interrupt_exit_code(work: &Work, options: RunOptions) -> Option<i32> {
    let Work::Input(events) = work else {
        return None;
    };
    events
        .iter()
        .any(|event| {
            let Event::Key(pressed) = event else {
                return false;
            };
            let emergency = options.emergency_exit == Some(*pressed);
            let interrupt = options.interrupt_policy == InterruptPolicy::Exit130
                && pressed.key == key::KeyCode::Char('c')
                && pressed.mods.ctrl;
            emergency || interrupt
        })
        .then_some(130)
}

/// Restore an intercepted terminal session before any widget sees the key.
fn intercept_interrupt(
    session: &mut TerminalSession,
    work: &Work,
    options: RunOptions,
) -> Result<Option<i32>> {
    let code = interrupt_exit_code(work, options);
    if code.is_some() {
        session.stop()?;
    }
    Ok(code)
}

/// Map IO results into canopy errors.
fn translate_result<T>(e: io::Result<T>) -> Result<T> {
    e.map_err(error::Error::TerminalIo)
}

/// Restore the terminal, then print a heading and a node tree dump to stderr.
fn stop_and_dump(session: &mut TerminalSession, core: &Core, heading: &str) {
    if let Err(error) = session.stop() {
        eprintln!("terminal restore failed: {error}");
    }
    eprintln!("{heading}");
    match dump(core) {
        Ok(dump_str) => eprintln!("{dump_str}"),
        Err(dump_err) => eprintln!("Failed to dump node tree: {dump_err}"),
    }
}

/// Handle a render error by restoring the terminal and dumping the node tree.
fn handle_render_error(
    error: error::Error,
    core: &Core,
    session: &mut TerminalSession,
) -> error::Error {
    eprintln!("Render error: {error}");
    stop_and_dump(session, core, "\nNode tree dump:");
    error
}

/// Run the terminal adapter until the application requests an exit.
///
/// `options` decides how Ctrl+C and the emergency exit key stop the loop. With
/// the default options, Ctrl+C restores the terminal, dumps the node tree, and
/// returns status 130. Keyboard enhancement flags are enabled so escape codes
/// are unambiguous.
pub fn runloop(mut cnpy: Canopy, options: RunOptions) -> Result<i32> {
    let mut be = CrosstermRender::default();
    let mut session = TerminalSession::new(Box::new(CrosstermControl::new()))?;

    let rx = cnpy
        .event_rx
        .take()
        .ok_or_else(|| error::Error::InvalidOperation("event loop already initialized".into()))?;

    let mut events = EventSource::new(cevent::EventStream::new(), rx);
    let size = translate_result(terminal::size())?;
    cnpy.set_root_size(Size::new(size.0.into(), size.1.into()))?;

    let runtime = Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| error::Error::RunLoop(format!("cannot start event runtime: {error}")))?;
    let _runtime_context = runtime.enter();

    let mut selector = WorkSelector::default();
    // The first turn prepares the initial frame.
    let mut work = Work::Prepare;
    loop {
        if let Some(code) = intercept_interrupt(&mut session, &work, options)? {
            stop_and_dump(
                &mut session,
                &cnpy.core,
                "\nTerminal interrupt - Node tree dump:",
            );
            return Ok(code);
        }

        let outcome = cnpy.turn(work)?;
        if let Some(code) = outcome.exit_code {
            return Ok(code);
        }
        if outcome.frame.is_some() {
            cnpy.emit_frame(&mut be)
                .map_err(|error| handle_render_error(error, &cnpy.core, &mut session))?;
        }
        work = runtime.block_on(selector.next(&mut cnpy, events.next_work()))?;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use futures::{channel::mpsc::unbounded, executor::block_on, stream};

    use super::*;
    use crate::{
        EvalRequest,
        backend::BackendControl,
        testing::{backend::TestRender, contracts},
    };

    #[test]
    fn shared_trace_crosses_terminal_ingestion_driver_and_emission() -> Result<()> {
        let mut canopy = contracts::app()?;
        canopy.turn(Work::Prepare)?;
        let mut backend = TestRender::new();
        canopy.emit_frame(&mut backend)?;
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([Ok(cevent::Event::Key(cevent::KeyEvent::new(
            cevent::KeyCode::Char('x'),
            cevent::KeyModifiers::empty(),
        )))]);
        let mut events = EventSource::new(terminal, rx);
        let work = block_on(WorkSelector::default().next(&mut canopy, events.next_work()))?;
        assert!(
            matches!(&work, Work::Input(events) if matches!(events.as_slice(), [Event::Key(_)]))
        );
        let outcome = canopy.turn(work)?;
        assert!(outcome.frame.is_some());
        canopy.emit_frame(&mut backend)?;
        assert_eq!(
            canopy.snapshot().unwrap().nodes[0]
                .semantics
                .value
                .as_deref(),
            Some("1")
        );
        assert_eq!(
            backend.text,
            ["111111111111", "111111111111", "111111111111"]
        );

        let request = EvalRequest {
            source: contracts::SCRIPT.into(),
            timeout: None,
            anchor: canopy.root_id(),
        };
        let mut outcome = canopy.turn(Work::StartEval(request))?;
        if outcome.completed.is_empty() {
            outcome = canopy.turn(Work::Wake)?;
        }
        assert_eq!(outcome.completed.len(), 1);
        assert_eq!(
            outcome.completed[0].result.as_ref().unwrap(),
            &contracts::expected()
        );
        canopy.emit_frame(&mut backend)?;
        assert_eq!(
            backend.text,
            ["999999999999", "999999999999", "999999999999"]
        );
        Ok(())
    }

    /// Backend lifecycle recorder used without acquiring a real terminal.
    #[derive(Debug)]
    struct PolicyBackend(Arc<AtomicUsize>);

    impl BackendControl for PolicyBackend {
        fn start(&mut self) -> Result<()> {
            Ok(())
        }
        fn stop(&mut self) -> Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Focusable terminal stand-in that records routed control keys.
    struct PolicyTerminal(Arc<AtomicUsize>);

    impl crate::Widget for PolicyTerminal {
        fn accept_focus(&self, _ctx: &dyn crate::ViewContext) -> bool {
            true
        }
        fn on_event(
            &mut self,
            event: &Event,
            _ctx: &mut dyn crate::Context,
        ) -> Result<crate::EventOutcome> {
            if matches!(event, Event::Key(_)) {
                self.0.fetch_add(1, Ordering::SeqCst);
                return Ok(crate::EventOutcome::Handle);
            }
            Ok(crate::EventOutcome::Ignore)
        }
    }

    #[test]
    fn adapter_interrupt_policy_routes_or_exits_and_restores_once() -> Result<()> {
        let control_c = key::Ctrl + 'c';
        let emergency = key::Ctrl + key::Alt + 'q';
        for (policy, pressed, expected_exit) in [
            (InterruptPolicy::Exit130, control_c, Some(130)),
            (InterruptPolicy::RouteToApplication, control_c, None),
            (InterruptPolicy::RouteToApplication, emergency, Some(130)),
            (InterruptPolicy::RouteToApplication, key::Ctrl + 'q', None),
        ] {
            let stops = Arc::new(AtomicUsize::new(0));
            let received = Arc::new(AtomicUsize::new(0));
            let mut session = TerminalSession::new(Box::new(PolicyBackend(stops.clone())))?;
            let mut canopy = Canopy::new();
            canopy.replace_root(PolicyTerminal(received.clone()))?;
            canopy.set_root_size(Size::new(8, 2))?;
            canopy.turn(Work::Prepare)?;
            let work = Work::Input(vec![Event::Key(pressed)]);
            let result = intercept_interrupt(
                &mut session,
                &work,
                RunOptions {
                    interrupt_policy: policy,
                    emergency_exit: Some(emergency),
                },
            )?;
            assert_eq!(result, expected_exit);
            if result.is_none() {
                canopy.turn(work)?;
            }
            assert_eq!(
                received.load(Ordering::SeqCst),
                usize::from(expected_exit.is_none())
            );
            drop(session);
            assert_eq!(stops.load(Ordering::SeqCst), 1);
        }
        assert_eq!(
            RunOptions::default().interrupt_policy,
            InterruptPolicy::Exit130
        );
        Ok(())
    }
}
