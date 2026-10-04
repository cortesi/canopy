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
    Canopy,
    backend::TerminalSession,
    core::{Core, canopy::TurnSelector, dump::dump},
    error,
    error::Result,
    geom::Size,
    input::{Event, key},
    runtime::TurnInput,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunOptions {
    /// Policy for Ctrl+C.
    pub interrupt_policy: InterruptPolicy,
    /// Optional exact key that exits even when Ctrl+C is routed.
    pub emergency_exit: Option<key::Key>,
    /// Whether an exit on Ctrl+C or the emergency key prints the node tree to
    /// standard error after it restores the terminal.
    pub interrupt_dump: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            interrupt_policy: InterruptPolicy::default(),
            emergency_exit: None,
            interrupt_dump: true,
        }
    }
}

/// Return whether a key is Ctrl+C.
fn is_control_c(pressed: key::Key) -> bool {
    pressed.key == key::KeyCode::Char('c') && pressed.mods.ctrl
}

/// Take the interrupts of a batch before it dispatches, and return the rest
/// of the batch and the exit status, if the host exits.
///
/// The emergency key always exits. Under `InterruptPolicy::Exit130`, the
/// application's interrupt hook takes each Ctrl+C first, and the host exits
/// on the first one that the hook declines. A taken Ctrl+C leaves the batch.
/// An exit restores the terminal before any event of the batch dispatches.
fn intercept_interrupt(
    session: &mut TerminalSession,
    canopy: &mut Canopy,
    work: TurnInput,
    options: RunOptions,
) -> Result<(TurnInput, Option<i32>)> {
    let TurnInput::Events(events) = work else {
        return Ok((work, None));
    };
    let mut kept = Vec::with_capacity(events.len());
    for event in events {
        if let Event::Key(pressed) = event {
            let emergency = options.emergency_exit == Some(pressed);
            let interrupt =
                options.interrupt_policy == InterruptPolicy::Exit130 && is_control_c(pressed);
            if interrupt && !emergency && canopy.take_interrupt() {
                continue;
            }
            if emergency || interrupt {
                session.stop()?;
                return Ok((TurnInput::Events(kept), Some(130)));
            }
        }
        kept.push(event);
    }
    Ok((TurnInput::Events(kept), None))
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
/// returns status 130. `RunOptions::interrupt_dump` turns the dump off, and an
/// interrupt hook (`Setup::on_interrupt`) can take a Ctrl+C instead.
/// Keyboard enhancement flags are enabled so escape codes are unambiguous.
pub fn runloop(mut cnpy: Canopy, options: RunOptions) -> Result<i32> {
    let mut be = CrosstermRender::default();
    let mut session = TerminalSession::new(Box::new(CrosstermControl::new()))?;

    let rx = cnpy
        .event_rx
        .take()
        .ok_or_else(|| error::Error::Invalid("event loop already initialized".into()))?;

    let mut events = EventSource::new(cevent::EventStream::new(), rx);
    let size = translate_result(terminal::size())?;
    cnpy.set_screen_size(Size::new(size.0.into(), size.1.into()))?;
    cnpy.set_motion_live(true);

    let runtime = Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| error::Error::Driver(format!("cannot start event runtime: {error}")))?;
    let _runtime_context = runtime.enter();

    let mut selector = TurnSelector::default();
    // The first turn prepares the initial frame.
    let mut work = TurnInput::Prepare;
    loop {
        let (rest, exit) = intercept_interrupt(&mut session, &mut cnpy, work, options)?;
        work = rest;
        if let Some(code) = exit {
            if options.interrupt_dump {
                stop_and_dump(
                    &mut session,
                    &cnpy.core,
                    "\nTerminal interrupt - Node tree dump:",
                );
            }
            return Ok(code);
        }

        let outcome = cnpy.turn(work)?;
        if let Some(code) = outcome.exit_code {
            return Ok(code);
        }
        if outcome.frame.is_some() || outcome.motion {
            cnpy.emit_frame(&mut be)
                .map_err(|error| handle_render_error(error, &cnpy.core, &mut session))?;
        }
        work = runtime.block_on(selector.next(&mut cnpy, events.next_input()))?;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use futures::{channel::mpsc, executor::block_on, stream};

    use super::*;
    use crate::{
        CanopyBuilder,
        backend::BackendControl,
        input::key::Key,
        script::EvalRequest,
        testing::{backend::TestRender, contracts},
    };

    #[test]
    fn shared_trace_crosses_terminal_ingestion_driver_and_emission() -> Result<()> {
        let mut canopy = contracts::app()?;
        canopy.turn(TurnInput::Prepare)?;
        let mut backend = TestRender::new();
        canopy.emit_frame(&mut backend)?;
        let (_tx, rx) = mpsc::unbounded();
        let terminal = stream::iter([Ok(cevent::Event::Key(cevent::KeyEvent::new(
            cevent::KeyCode::Char('x'),
            cevent::KeyModifiers::empty(),
        )))]);
        let mut events = EventSource::new(terminal, rx);
        let work = block_on(TurnSelector::default().next(&mut canopy, events.next_input()))?;
        assert!(
            matches!(&work, TurnInput::Events(events) if matches!(events.as_slice(), [Event::Key(_)]))
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
        };
        let mut outcome = canopy.turn(TurnInput::StartEval(request))?;
        if outcome.completed.is_empty() {
            outcome = canopy.turn(TurnInput::Wake)?;
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
        fn key_outcome(&self, _key: Key, _ctx: &dyn crate::ViewContext) -> crate::EventOutcome {
            crate::EventOutcome::Handle
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
            let mut canopy = CanopyBuilder::new().build()?;
            canopy.replace_root(PolicyTerminal(received.clone()))?;
            canopy.set_screen_size(Size::new(8, 2))?;
            canopy.turn(TurnInput::Prepare)?;
            let work = TurnInput::Events(vec![Event::Key(pressed)]);
            let (work, result) = intercept_interrupt(
                &mut session,
                &mut canopy,
                work,
                RunOptions {
                    interrupt_policy: policy,
                    emergency_exit: Some(emergency),
                    ..RunOptions::default()
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
        assert!(RunOptions::default().interrupt_dump);
        Ok(())
    }

    /// Interrupts that `take_first_interrupt` has seen.
    static OFFERED: AtomicUsize = AtomicUsize::new(0);

    /// Take the first interrupt, and decline every later one.
    fn take_first_interrupt(_context: &mut dyn crate::Context) -> Result<bool> {
        Ok(OFFERED.fetch_add(1, Ordering::SeqCst) == 0)
    }

    #[test]
    fn an_interrupt_hook_takes_ctrl_c_until_it_declines() -> Result<()> {
        let control_c = key::Ctrl + 'c';
        let emergency = key::Ctrl + key::Alt + 'q';
        let options = RunOptions {
            emergency_exit: Some(emergency),
            ..RunOptions::default()
        };
        let stops = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(AtomicUsize::new(0));
        let mut session = TerminalSession::new(Box::new(PolicyBackend(stops.clone())))?;
        let mut canopy = CanopyBuilder::new()
            .configure(|setup| {
                setup.on_interrupt(take_first_interrupt);
                Ok(())
            })
            .build()?;
        canopy.replace_root(PolicyTerminal(received.clone()))?;
        canopy.set_screen_size(Size::new(8, 2))?;
        canopy.turn(TurnInput::Prepare)?;

        // The hook takes the first Ctrl+C, and the rest of the batch runs.
        let batch = TurnInput::Events(vec![Event::Key(control_c), Event::Key(key::Key::from('x'))]);
        let (rest, exit) = intercept_interrupt(&mut session, &mut canopy, batch, options)?;
        assert_eq!(exit, None);
        assert!(matches!(&rest, TurnInput::Events(events) if events.len() == 1));
        canopy.turn(rest)?;
        assert_eq!(
            received.load(Ordering::SeqCst),
            1,
            "Ctrl+C reached no widget"
        );

        // The emergency key exits without asking the hook.
        let (_, exit) = intercept_interrupt(
            &mut session,
            &mut canopy,
            TurnInput::Events(vec![Event::Key(emergency)]),
            options,
        )?;
        assert_eq!(exit, Some(130));
        assert_eq!(OFFERED.load(Ordering::SeqCst), 1);

        // A declined Ctrl+C exits.
        let (_, exit) = intercept_interrupt(
            &mut session,
            &mut canopy,
            TurnInput::Events(vec![Event::Key(control_c)]),
            options,
        )?;
        assert_eq!(exit, Some(130));
        assert_eq!(OFFERED.load(Ordering::SeqCst), 2);
        drop(session);
        assert_eq!(
            stops.load(Ordering::SeqCst),
            1,
            "the terminal restores once"
        );
        Ok(())
    }
}
