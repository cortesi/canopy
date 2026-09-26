//! Terminal session: acquire and release terminal capabilities.

use std::io::{self, Stderr};

use crossterm::{ExecutableCommand, cursor as ccursor, event as cevent, terminal};

use super::translate_result;
use crate::{backend::BackendControl, error::Result};

/// Terminal operations needed to acquire and restore a session.
trait TerminalOperations {
    /// Enable terminal raw mode.
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    /// Disable terminal raw mode.
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    /// Enter the alternate screen.
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    /// Leave the alternate screen.
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    /// Enable mouse capture.
    fn enable_mouse_capture(&mut self) -> io::Result<()>;
    /// Disable mouse capture.
    fn disable_mouse_capture(&mut self) -> io::Result<()>;
    /// Hide the cursor.
    fn hide_cursor(&mut self) -> io::Result<()>;
    /// Show the cursor.
    fn show_cursor(&mut self) -> io::Result<()>;
    /// Push keyboard enhancement flags.
    fn push_keyboard_enhancements(&mut self) -> io::Result<()>;
    /// Pop keyboard enhancement flags.
    fn pop_keyboard_enhancements(&mut self) -> io::Result<()>;
    /// Enable terminal focus change reporting.
    fn enable_focus_change(&mut self) -> io::Result<()>;
    /// Disable terminal focus change reporting.
    fn disable_focus_change(&mut self) -> io::Result<()>;
}

impl TerminalOperations for Stderr {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        self.execute(terminal::EnterAlternateScreen).map(|_| ())
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        self.execute(terminal::LeaveAlternateScreen).map(|_| ())
    }

    fn enable_mouse_capture(&mut self) -> io::Result<()> {
        self.execute(cevent::EnableMouseCapture).map(|_| ())
    }

    fn disable_mouse_capture(&mut self) -> io::Result<()> {
        self.execute(cevent::DisableMouseCapture).map(|_| ())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.execute(ccursor::Hide).map(|_| ())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.execute(ccursor::Show).map(|_| ())
    }

    fn push_keyboard_enhancements(&mut self) -> io::Result<()> {
        self.execute(cevent::PushKeyboardEnhancementFlags(
            cevent::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
        ))
        .map(|_| ())
    }

    fn pop_keyboard_enhancements(&mut self) -> io::Result<()> {
        self.execute(cevent::PopKeyboardEnhancementFlags)
            .map(|_| ())
    }

    fn enable_focus_change(&mut self) -> io::Result<()> {
        self.execute(cevent::EnableFocusChange).map(|_| ())
    }

    fn disable_focus_change(&mut self) -> io::Result<()> {
        self.execute(cevent::DisableFocusChange).map(|_| ())
    }
}

/// Terminal capabilities currently owned by one controller.
#[derive(Debug, Default)]
struct TerminalCapabilities {
    /// Whether raw mode is active.
    raw_mode_enabled: bool,
    /// Whether the alternate screen is active.
    alternate_screen_entered: bool,
    /// Whether mouse capture is active.
    mouse_capture_enabled: bool,
    /// Whether the cursor is hidden.
    cursor_hidden: bool,
    /// Whether keyboard enhancement flags were pushed.
    keyboard_enhancements_pushed: bool,
    /// Whether focus change reporting is enabled.
    focus_change_enabled: bool,
}

impl TerminalCapabilities {
    /// Return whether any terminal capability remains acquired.
    fn is_active(&self) -> bool {
        self.raw_mode_enabled
            || self.alternate_screen_entered
            || self.mouse_capture_enabled
            || self.cursor_hidden
            || self.keyboard_enhancements_pushed
            || self.focus_change_enabled
    }
}

/// Acquire terminal capabilities in dependency order.
fn acquire_terminal(
    terminal: &mut impl TerminalOperations,
    capabilities: &mut TerminalCapabilities,
) -> io::Result<()> {
    terminal.enable_raw_mode()?;
    capabilities.raw_mode_enabled = true;
    terminal.enter_alternate_screen()?;
    capabilities.alternate_screen_entered = true;
    terminal.enable_mouse_capture()?;
    capabilities.mouse_capture_enabled = true;
    terminal.hide_cursor()?;
    capabilities.cursor_hidden = true;
    terminal.push_keyboard_enhancements()?;
    capabilities.keyboard_enhancements_pushed = true;
    terminal.enable_focus_change()?;
    capabilities.focus_change_enabled = true;
    Ok(())
}

/// Record a capability-release result while retaining the first error.
fn record_release(result: io::Result<()>, active: &mut bool, first_error: &mut Option<io::Error>) {
    match result {
        Ok(()) => *active = false,
        Err(error) if first_error.is_none() => *first_error = Some(error),
        Err(_) => {}
    }
}

/// Release every acquired terminal capability in reverse order.
fn release_terminal(
    terminal: &mut impl TerminalOperations,
    capabilities: &mut TerminalCapabilities,
) -> io::Result<()> {
    let mut first_error = None;
    if capabilities.focus_change_enabled {
        record_release(
            terminal.disable_focus_change(),
            &mut capabilities.focus_change_enabled,
            &mut first_error,
        );
    }
    if capabilities.keyboard_enhancements_pushed {
        record_release(
            terminal.pop_keyboard_enhancements(),
            &mut capabilities.keyboard_enhancements_pushed,
            &mut first_error,
        );
    }
    if capabilities.cursor_hidden {
        record_release(
            terminal.show_cursor(),
            &mut capabilities.cursor_hidden,
            &mut first_error,
        );
    }
    if capabilities.mouse_capture_enabled {
        record_release(
            terminal.disable_mouse_capture(),
            &mut capabilities.mouse_capture_enabled,
            &mut first_error,
        );
    }
    if capabilities.alternate_screen_entered {
        record_release(
            terminal.leave_alternate_screen(),
            &mut capabilities.alternate_screen_entered,
            &mut first_error,
        );
    }
    if capabilities.raw_mode_enabled {
        record_release(
            terminal.disable_raw_mode(),
            &mut capabilities.raw_mode_enabled,
            &mut first_error,
        );
    }
    first_error.map_or(Ok(()), Err)
}

/// Crossterm-backed implementation of `BackendControl`.
#[derive(Debug)]
pub struct CrosstermControl {
    /// Stderr handle used for terminal operations.
    terminal: Stderr,
    /// Capabilities currently owned by the controller.
    capabilities: TerminalCapabilities,
}

impl CrosstermControl {
    /// Build a crossterm controller.
    pub fn new() -> Self {
        Self {
            terminal: io::stderr(),
            capabilities: TerminalCapabilities::default(),
        }
    }

    /// Enter alternate screen and raw mode, rolling back a partial start.
    fn enter(&mut self) -> io::Result<()> {
        if self.capabilities.is_active() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "terminal backend is already active",
            ));
        }
        if let Err(error) = acquire_terminal(&mut self.terminal, &mut self.capabilities) {
            drop(release_terminal(&mut self.terminal, &mut self.capabilities));
            return Err(error);
        }
        Ok(())
    }

    /// Leave alternate screen and restore terminal state.
    fn exit(&mut self) -> io::Result<()> {
        release_terminal(&mut self.terminal, &mut self.capabilities)
    }
}

impl BackendControl for CrosstermControl {
    fn start(&mut self) -> Result<()> {
        translate_result(self.enter())
    }
    fn stop(&mut self) -> Result<()> {
        translate_result(self.exit())
    }
}

impl Drop for CrosstermControl {
    fn drop(&mut self) {
        drop(self.exit());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fault-injecting terminal used to verify acquisition rollback.
    #[derive(Debug, Default)]
    struct FakeTerminal {
        calls: Vec<&'static str>,
        fail_at: Option<usize>,
        acquisitions: usize,
    }

    impl FakeTerminal {
        /// Record an acquisition and fail at the configured step.
        fn acquire(&mut self, name: &'static str) -> io::Result<()> {
            self.calls.push(name);
            self.acquisitions += 1;
            if self.fail_at == Some(self.acquisitions) {
                return Err(io::Error::other("injected terminal failure"));
            }
            Ok(())
        }

        /// Record an infallible release.
        fn release(&mut self, name: &'static str) -> io::Result<()> {
            self.calls.push(name);
            Ok(())
        }
    }

    impl TerminalOperations for FakeTerminal {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.acquire("raw+")
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.release("raw-")
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.acquire("screen+")
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.release("screen-")
        }

        fn enable_mouse_capture(&mut self) -> io::Result<()> {
            self.acquire("mouse+")
        }

        fn disable_mouse_capture(&mut self) -> io::Result<()> {
            self.release("mouse-")
        }

        fn hide_cursor(&mut self) -> io::Result<()> {
            self.acquire("cursor-")
        }

        fn show_cursor(&mut self) -> io::Result<()> {
            self.release("cursor+")
        }

        fn push_keyboard_enhancements(&mut self) -> io::Result<()> {
            self.acquire("keyboard+")
        }

        fn pop_keyboard_enhancements(&mut self) -> io::Result<()> {
            self.release("keyboard-")
        }

        fn enable_focus_change(&mut self) -> io::Result<()> {
            self.acquire("focus+")
        }

        fn disable_focus_change(&mut self) -> io::Result<()> {
            self.release("focus-")
        }
    }

    #[test]
    fn terminal_capabilities_balance_in_reverse_order() -> io::Result<()> {
        let mut terminal = FakeTerminal::default();
        let mut capabilities = TerminalCapabilities::default();

        acquire_terminal(&mut terminal, &mut capabilities)?;
        release_terminal(&mut terminal, &mut capabilities)?;

        assert_eq!(
            terminal.calls,
            [
                "raw+",
                "screen+",
                "mouse+",
                "cursor-",
                "keyboard+",
                "focus+",
                "focus-",
                "keyboard-",
                "cursor+",
                "mouse-",
                "screen-",
                "raw-",
            ]
        );
        assert!(!capabilities.is_active());
        Ok(())
    }

    #[test]
    fn every_partial_terminal_start_restores_acquired_capabilities() {
        for fail_at in 1..=6 {
            let mut terminal = FakeTerminal {
                fail_at: Some(fail_at),
                ..FakeTerminal::default()
            };
            let mut capabilities = TerminalCapabilities::default();

            acquire_terminal(&mut terminal, &mut capabilities)
                .expect_err("configured acquisition should fail");
            release_terminal(&mut terminal, &mut capabilities)
                .expect("rollback should release every acquired capability");

            assert!(!capabilities.is_active(), "failed at step {fail_at}");
        }
    }
}
