use std::{
    io::{self, Stderr, Write},
    mem, panic,
};

use color_backtrace::{BacktracePrinter, default_output_stream};
use futures::{
    FutureExt,
    channel::mpsc::UnboundedReceiver,
    executor::block_on,
    future::{Either, select},
    pin_mut,
    stream::{Stream, StreamExt},
};
use scopeguard::guard;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Canopy, NodeId,
    backend::{BackendControl, TerminalSession},
    core::{Core, dump::dump, text},
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Point, Size},
    render::RenderBackend,
    style::{Color, ResolvedStyle},
};
/// Simple event source wrapper for receiving events.
///
/// This coalesces consecutive mouse-move events so clicks are not delayed by move bursts.
struct EventSource<S> {
    /// Cancellable terminal event stream owned by the run loop.
    terminal: S,
    /// Framework event receiver channel.
    internal: UnboundedReceiver<Event>,
    /// Buffered non-move event encountered while coalescing.
    pending: Option<Event>,
}

impl<S> EventSource<S>
where
    S: Stream<Item = io::Result<cevent::Event>> + Unpin,
{
    /// Construct a new event source.
    fn new(terminal: S, internal: UnboundedReceiver<Event>) -> Self {
        Self {
            terminal,
            internal,
            pending: None,
        }
    }

    /// Await one event from either the terminal or the framework channel.
    async fn next_uncoalesced(&mut self) -> Result<Event> {
        let terminal = self.terminal.next();
        let internal = self.internal.next();
        pin_mut!(terminal, internal);
        match select(terminal, internal).await {
            Either::Left((terminal, _)) => terminal_event(terminal),
            Either::Right((Some(event), _)) => Ok(event),
            Either::Right((None, _)) => Err(error::Error::RunLoop(
                "framework event channel closed".into(),
            )),
        }
    }

    /// Take one event that is already available without waiting.
    fn next_ready(&mut self) -> Result<Option<Event>> {
        if let Some(internal) = self.internal.next().now_or_never() {
            return internal
                .map(Some)
                .ok_or_else(|| error::Error::RunLoop("framework event channel closed".into()));
        }
        if let Some(terminal) = self.terminal.next().now_or_never() {
            return terminal_event(terminal).map(Some);
        }
        Ok(None)
    }

    /// Await the next event, coalescing consecutive ready mouse moves.
    async fn next(&mut self) -> Result<Event> {
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }

        let mut event = self.next_uncoalesced().await?;
        if matches!(
            event,
            Event::Mouse(mouse::MouseEvent {
                action: mouse::Action::Moved,
                ..
            })
        ) {
            while let Some(next) = self.next_ready()? {
                if matches!(
                    next,
                    Event::Mouse(mouse::MouseEvent {
                        action: mouse::Action::Moved,
                        ..
                    })
                ) {
                    event = next;
                } else {
                    self.pending = Some(next);
                    break;
                }
            }
        }

        Ok(event)
    }
}

/// Translate one terminal stream item or report reader termination.
fn terminal_event(event: Option<io::Result<cevent::Event>>) -> Result<Event> {
    match event {
        Some(Ok(event)) => Ok(translate_event(event)),
        Some(Err(error)) => Err(error::Error::TerminalIo(error)),
        None => Err(error::Error::RunLoop("terminal event stream closed".into())),
    }
}

use crossterm::{
    self, ExecutableCommand, QueueableCommand, cursor as ccursor, event as cevent, style, terminal,
};

/// Translate a canopy color into a crossterm color.
fn translate_color(c: Color) -> style::Color {
    match c {
        Color::Black => style::Color::Black,
        Color::DarkGrey => style::Color::DarkGrey,
        Color::Red => style::Color::Red,
        Color::DarkRed => style::Color::DarkRed,
        Color::Green => style::Color::Green,
        Color::DarkGreen => style::Color::DarkGreen,
        Color::Yellow => style::Color::Yellow,
        Color::DarkYellow => style::Color::DarkYellow,
        Color::Blue => style::Color::Blue,
        Color::DarkBlue => style::Color::DarkBlue,
        Color::Magenta => style::Color::Magenta,
        Color::DarkMagenta => style::Color::DarkMagenta,
        Color::Cyan => style::Color::Cyan,
        Color::DarkCyan => style::Color::DarkCyan,
        Color::White => style::Color::White,
        Color::Grey => style::Color::Grey,
        Color::Rgb { r, g, b } => style::Color::Rgb { r, g, b },
        Color::AnsiValue(a) => style::Color::AnsiValue(a),
    }
}

/// Map IO results into canopy errors.
fn translate_result<T>(e: io::Result<T>) -> Result<T> {
    match e {
        Ok(t) => Ok(t),
        Err(error) => Err(error::Error::TerminalIo(error)),
    }
}

/// Terminal operations needed to acquire and restore a session.
trait TerminalOperations: io::Write {
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
}

impl TerminalCapabilities {
    /// Return whether any terminal capability remains acquired.
    fn is_active(&self) -> bool {
        self.raw_mode_enabled
            || self.alternate_screen_entered
            || self.mouse_capture_enabled
            || self.cursor_hidden
            || self.keyboard_enhancements_pushed
    }
}

/// Acquire terminal capabilities in dependency order.
fn acquire_terminal(
    terminal: &mut impl TerminalOperations,
    capabilities: &mut TerminalCapabilities,
    keyboard_enhancements: bool,
) -> io::Result<()> {
    terminal.enable_raw_mode()?;
    capabilities.raw_mode_enabled = true;
    terminal.enter_alternate_screen()?;
    capabilities.alternate_screen_entered = true;
    terminal.enable_mouse_capture()?;
    capabilities.mouse_capture_enabled = true;
    terminal.hide_cursor()?;
    capabilities.cursor_hidden = true;
    if keyboard_enhancements {
        terminal.push_keyboard_enhancements()?;
        capabilities.keyboard_enhancements_pushed = true;
    }
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
    /// Whether to enable keyboard enhancement flags on startup.
    enable_keyboard_enhancements: bool,
}

impl CrosstermControl {
    /// Build a crossterm controller with keyboard enhancements enabled or disabled.
    pub fn new(enable_keyboard_enhancements: bool) -> Self {
        Self {
            terminal: io::stderr(),
            capabilities: TerminalCapabilities::default(),
            enable_keyboard_enhancements,
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
        if let Err(error) = acquire_terminal(
            &mut self.terminal,
            &mut self.capabilities,
            self.enable_keyboard_enhancements,
        ) {
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

impl Default for CrosstermControl {
    fn default() -> Self {
        Self::new(true)
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

/// Crossterm-backed render backend.
pub struct CrosstermRender {
    /// Stderr handle used for rendering output.
    fp: Stderr,
}

impl CrosstermRender {
    /// Flush pending output.
    fn flush(&mut self) -> io::Result<()> {
        self.fp.flush()?;
        Ok(())
    }

    /// Apply a style to subsequent output.
    fn apply_style(&mut self, s: &ResolvedStyle) -> io::Result<()> {
        // Always reset first to clear any previous attributes, then set colors and attrs.
        // Order is important: reset clears everything, so we must set colors after.
        self.fp
            .queue(style::SetAttribute(style::Attribute::Reset))?;
        self.fp
            .queue(style::SetForegroundColor(translate_color(s.fg)))?;
        self.fp
            .queue(style::SetBackgroundColor(translate_color(s.bg)))?;

        // Now add the desired attributes
        if s.attrs.bold {
            self.fp.queue(style::SetAttribute(style::Attribute::Bold))?;
        }
        if s.attrs.crossedout {
            self.fp
                .queue(style::SetAttribute(style::Attribute::CrossedOut))?;
        }
        if s.attrs.dim {
            self.fp.queue(style::SetAttribute(style::Attribute::Dim))?;
        }
        if s.attrs.italic {
            self.fp
                .queue(style::SetAttribute(style::Attribute::Italic))?;
        }
        if s.attrs.overline {
            self.fp
                .queue(style::SetAttribute(style::Attribute::OverLined))?;
        }
        if s.attrs.underline {
            self.fp
                .queue(style::SetAttribute(style::Attribute::Underlined))?;
        }
        Ok(())
    }

    /// Write text at a position.
    fn text(&mut self, loc: Point, txt: &str) -> io::Result<()> {
        for run in positioned_text_runs(loc, txt) {
            let x = u16::try_from(run.location.x).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal x coordinate exceeds u16",
                )
            })?;
            let y = u16::try_from(run.location.y).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal y coordinate exceeds u16",
                )
            })?;
            self.fp.queue(ccursor::MoveTo(x, y))?;
            self.fp.queue(style::Print(run.text))?;
        }
        Ok(())
    }
}

/// A string fragment with an absolute terminal-cell location.
#[derive(Debug, PartialEq, Eq)]
struct PositionedTextRun {
    /// Location where the text run should be printed.
    location: Point,
    /// Text to print at the location.
    text: String,
}

/// Split text into absolute-positioned runs at wide grapheme boundaries.
fn positioned_text_runs(loc: Point, txt: &str) -> Vec<PositionedTextRun> {
    let mut runs = Vec::new();
    let mut run = String::new();
    let mut run_x = loc.x;
    let mut x = loc.x;

    for grapheme in txt.graphemes(true) {
        let width = text::grapheme_width(grapheme);
        if width == 0 {
            continue;
        }

        if width > 1 {
            push_positioned_text_run(&mut runs, Point { x: run_x, y: loc.y }, &mut run);
            runs.push(PositionedTextRun {
                location: Point { x, y: loc.y },
                text: grapheme.to_string(),
            });
            x = x.saturating_add(width as u32);
            run_x = x;
            continue;
        }

        if run.is_empty() {
            run_x = x;
        }
        run.push_str(grapheme);
        x = x.saturating_add(width as u32);
    }

    push_positioned_text_run(&mut runs, Point { x: run_x, y: loc.y }, &mut run);
    runs
}

/// Add a non-empty positioned text run.
fn push_positioned_text_run(runs: &mut Vec<PositionedTextRun>, location: Point, text: &mut String) {
    if text.is_empty() {
        return;
    }
    runs.push(PositionedTextRun {
        location,
        text: mem::take(text),
    });
}

impl Default for CrosstermRender {
    fn default() -> Self {
        Self { fp: io::stderr() }
    }
}

impl RenderBackend for CrosstermRender {
    fn flush(&mut self) -> Result<()> {
        translate_result(self.flush())
    }

    fn style(&mut self, s: &ResolvedStyle) -> Result<()> {
        translate_result(self.apply_style(s))
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        translate_result(self.text(loc, txt))
    }

    fn supports_char_shift(&self) -> bool {
        true
    }

    fn supports_line_shift(&self) -> bool {
        true
    }

    fn shift_chars(&mut self, loc: Point, count: i32) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        translate_result(self.fp.queue(ccursor::MoveTo(loc.x as u16, loc.y as u16)))?;
        let seq = if count > 0 {
            format!("\x1b[{count_abs}@")
        } else {
            format!("\x1b[{count_abs}P")
        };
        translate_result(self.fp.queue(style::Print(seq)))?;
        Ok(())
    }

    fn shift_lines(&mut self, _top: u32, _bottom: u32, count: i32) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let top = _top.min(u16::MAX as u32) as u16;
        let bottom = _bottom.min(u16::MAX as u32) as u16;
        if top > bottom {
            return Ok(());
        }
        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        let region = format!("\x1b[{};{}r", top + 1, bottom + 1);
        translate_result(self.fp.queue(style::Print(region)))?;
        translate_result(self.fp.queue(ccursor::MoveTo(0, top)))?;
        let seq = if count > 0 {
            format!("\x1b[{count_abs}T")
        } else {
            format!("\x1b[{count_abs}S")
        };
        translate_result(self.fp.queue(style::Print(seq)))?;
        translate_result(self.fp.queue(style::Print("\x1b[r")))?;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Translate crossterm key modifiers into canopy modifiers.
fn translate_key_modifiers(mods: cevent::KeyModifiers) -> key::Mods {
    key::Mods {
        shift: mods.contains(cevent::KeyModifiers::SHIFT),
        ctrl: mods.contains(cevent::KeyModifiers::CONTROL),
        alt: mods.contains(cevent::KeyModifiers::ALT),
    }
}

/// Translate a crossterm mouse button into a canopy button.
fn translate_button(b: cevent::MouseButton) -> mouse::Button {
    match b {
        cevent::MouseButton::Left => mouse::Button::Left,
        cevent::MouseButton::Right => mouse::Button::Right,
        cevent::MouseButton::Middle => mouse::Button::Middle,
    }
}

/// Translate a crossterm event into a canopy event.
fn translate_event(e: cevent::Event) -> Event {
    match e {
        cevent::Event::Key(k) => Event::Key(key::Key {
            mods: translate_key_modifiers(k.modifiers),
            key: match k.code {
                cevent::KeyCode::Backspace => key::KeyCode::Backspace,
                cevent::KeyCode::Enter => key::KeyCode::Enter,
                cevent::KeyCode::Left => key::KeyCode::Left,
                cevent::KeyCode::Right => key::KeyCode::Right,
                cevent::KeyCode::Up => key::KeyCode::Up,
                cevent::KeyCode::Down => key::KeyCode::Down,
                cevent::KeyCode::Home => key::KeyCode::Home,
                cevent::KeyCode::End => key::KeyCode::End,
                cevent::KeyCode::PageUp => key::KeyCode::PageUp,
                cevent::KeyCode::PageDown => key::KeyCode::PageDown,
                cevent::KeyCode::Tab => key::KeyCode::Tab,
                cevent::KeyCode::BackTab => key::KeyCode::BackTab,
                cevent::KeyCode::Delete => key::KeyCode::Delete,
                cevent::KeyCode::Insert => key::KeyCode::Insert,
                cevent::KeyCode::F(x) => key::KeyCode::F(x),
                cevent::KeyCode::Char(c) => key::KeyCode::Char(c),
                cevent::KeyCode::Null => key::KeyCode::Null,
                cevent::KeyCode::Esc => key::KeyCode::Esc,
                cevent::KeyCode::CapsLock => key::KeyCode::CapsLock,
                cevent::KeyCode::ScrollLock => key::KeyCode::ScrollLock,
                cevent::KeyCode::NumLock => key::KeyCode::NumLock,
                cevent::KeyCode::PrintScreen => key::KeyCode::PrintScreen,
                cevent::KeyCode::Pause => key::KeyCode::Pause,
                cevent::KeyCode::Menu => key::KeyCode::Menu,
                cevent::KeyCode::KeypadBegin => key::KeyCode::KeypadBegin,
                cevent::KeyCode::Media(k) => key::KeyCode::Media(match k {
                    cevent::MediaKeyCode::Play => key::MediaKeyCode::Play,
                    cevent::MediaKeyCode::Pause => key::MediaKeyCode::Pause,
                    cevent::MediaKeyCode::PlayPause => key::MediaKeyCode::PlayPause,
                    cevent::MediaKeyCode::Reverse => key::MediaKeyCode::Reverse,
                    cevent::MediaKeyCode::Stop => key::MediaKeyCode::Stop,
                    cevent::MediaKeyCode::FastForward => key::MediaKeyCode::FastForward,
                    cevent::MediaKeyCode::Rewind => key::MediaKeyCode::Rewind,
                    cevent::MediaKeyCode::TrackNext => key::MediaKeyCode::TrackNext,
                    cevent::MediaKeyCode::TrackPrevious => key::MediaKeyCode::TrackPrevious,
                    cevent::MediaKeyCode::Record => key::MediaKeyCode::Record,
                    cevent::MediaKeyCode::LowerVolume => key::MediaKeyCode::LowerVolume,
                    cevent::MediaKeyCode::RaiseVolume => key::MediaKeyCode::RaiseVolume,
                    cevent::MediaKeyCode::MuteVolume => key::MediaKeyCode::MuteVolume,
                }),
                cevent::KeyCode::Modifier(m) => key::KeyCode::Modifier(match m {
                    cevent::ModifierKeyCode::LeftShift => key::ModifierKeyCode::LeftShift,
                    cevent::ModifierKeyCode::LeftControl => key::ModifierKeyCode::LeftControl,
                    cevent::ModifierKeyCode::LeftAlt => key::ModifierKeyCode::LeftAlt,
                    cevent::ModifierKeyCode::LeftSuper => key::ModifierKeyCode::LeftSuper,
                    cevent::ModifierKeyCode::LeftHyper => key::ModifierKeyCode::LeftHyper,
                    cevent::ModifierKeyCode::LeftMeta => key::ModifierKeyCode::LeftMeta,
                    cevent::ModifierKeyCode::RightShift => key::ModifierKeyCode::RightShift,
                    cevent::ModifierKeyCode::RightControl => key::ModifierKeyCode::RightControl,
                    cevent::ModifierKeyCode::RightAlt => key::ModifierKeyCode::RightAlt,
                    cevent::ModifierKeyCode::RightSuper => key::ModifierKeyCode::RightSuper,
                    cevent::ModifierKeyCode::RightHyper => key::ModifierKeyCode::RightHyper,
                    cevent::ModifierKeyCode::RightMeta => key::ModifierKeyCode::RightMeta,
                    cevent::ModifierKeyCode::IsoLevel3Shift => key::ModifierKeyCode::IsoLevel3Shift,
                    cevent::ModifierKeyCode::IsoLevel5Shift => key::ModifierKeyCode::IsoLevel5Shift,
                }),
            },
        }),
        cevent::Event::Mouse(m) => {
            let mut button = mouse::Button::None;
            let action = match m.kind {
                cevent::MouseEventKind::Down(b) => {
                    button = translate_button(b);
                    mouse::Action::Down
                }
                cevent::MouseEventKind::Up(b) => {
                    button = translate_button(b);
                    mouse::Action::Up
                }
                cevent::MouseEventKind::Drag(b) => {
                    button = translate_button(b);
                    mouse::Action::Drag
                }
                cevent::MouseEventKind::Moved => mouse::Action::Moved,
                cevent::MouseEventKind::ScrollDown => mouse::Action::ScrollDown,
                cevent::MouseEventKind::ScrollUp => mouse::Action::ScrollUp,
                cevent::MouseEventKind::ScrollLeft => mouse::Action::ScrollLeft,
                cevent::MouseEventKind::ScrollRight => mouse::Action::ScrollRight,
            };
            Event::Mouse(mouse::MouseEvent {
                button,
                action,
                location: Point {
                    x: m.column.into(),
                    y: m.row.into(),
                },
                modifiers: translate_key_modifiers(m.modifiers),
            })
        }
        cevent::Event::Resize(x, y) => Event::Resize(Size::new(x.into(), y.into())),
        cevent::Event::FocusGained => Event::FocusGained,
        cevent::Event::FocusLost => Event::FocusLost,
        cevent::Event::Paste(s) => Event::Paste(s),
    }
}

/// Helper function to handle render errors by exiting alternate screen mode
/// and displaying the error with a node tree dump
fn handle_render_error(
    error: error::Error,
    core: &Core,
    root: NodeId,
    focus: Option<NodeId>,
    session: &TerminalSession,
) -> error::Error {
    drop(session.stop());

    // Print error and node dump
    eprintln!("Render error: {error}");
    eprintln!("\nNode tree dump:");
    match dump(core, root, focus) {
        Ok(dump_str) => eprintln!("{dump_str}"),
        Err(dump_err) => eprintln!("Failed to dump node tree: {dump_err}"),
    }

    error
}

/// Ctrl+C handling policy for the crossterm runloop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlCBehavior {
    /// Stop the runloop with status 130.
    Exit,
    /// Dump the node tree and stop the runloop with status 130.
    DumpTreeAndExit,
}

/// Options for configuring the crossterm runloop behavior.
#[derive(Debug, Clone, Copy)]
pub struct RunloopOptions {
    /// Install a panic hook that restores the terminal before printing a backtrace.
    pub install_panic_hook: bool,
    /// Configure how Ctrl+C is handled.
    pub ctrl_c: CtrlCBehavior,
    /// Enable keyboard enhancement flags for disambiguated escape codes.
    pub enable_keyboard_enhancements: bool,
}

impl RunloopOptions {
    /// Construct options that dump the node tree before exiting on Ctrl+C.
    pub fn ctrlc_dump() -> Self {
        Self {
            ctrl_c: CtrlCBehavior::DumpTreeAndExit,
            ..Self::default()
        }
    }
}

impl Default for RunloopOptions {
    fn default() -> Self {
        Self {
            install_panic_hook: false,
            ctrl_c: CtrlCBehavior::Exit,
            enable_keyboard_enhancements: true,
        }
    }
}

/// Run the main render/event loop using the crossterm backend.
pub fn runloop(cnpy: Canopy) -> Result<i32> {
    runloop_with_options(cnpy, RunloopOptions::default())
}

/// Run the main render/event loop using the crossterm backend with custom options.
pub fn runloop_with_options(mut cnpy: Canopy, options: RunloopOptions) -> Result<i32> {
    let mut be = CrosstermRender::default();
    cnpy.register_backend(CrosstermControl::new(options.enable_keyboard_enhancements));
    let backend = cnpy
        .backend
        .take()
        .ok_or_else(|| error::Error::Internal("backend not set".into()))?;
    let session = TerminalSession::new(backend)?;

    let _panic_hook = if options.install_panic_hook {
        let previous = panic::take_hook();
        let cleanup = session.cleanup();
        panic::set_hook(Box::new(move |pi| {
            drop(cleanup.stop());
            drop(BacktracePrinter::new().print_panic_info(pi, &mut default_output_stream()));
        }));
        Some(guard(previous, |hook| {
            panic::set_hook(hook);
        }))
    } else {
        None
    };

    let rx = cnpy
        .event_rx
        .take()
        .ok_or_else(|| error::Error::InvalidOperation("event loop already initialized".into()))?;

    let mut events = EventSource::new(cevent::EventStream::new(), rx);
    let size = translate_result(terminal::size())?;
    cnpy.set_root_size(Size::new(size.0.into(), size.1.into()))?;

    if let Err(e) = cnpy.render(&mut be) {
        return Err(handle_render_error(
            e,
            &cnpy.core,
            cnpy.core.root,
            cnpy.core.focus,
            &session,
        ));
    }
    translate_result(be.flush())?;
    if let Some(code) = cnpy.core.take_exit_request() {
        return Ok(code);
    }

    loop {
        let event = block_on(events.next())?;

        if matches!(
            &event,
            Event::Key(key::Key {
                key: key::KeyCode::Char('c'),
                mods: key::Mods { ctrl: true, .. },
            })
        ) {
            drop(session.stop());
            if options.ctrl_c == CtrlCBehavior::DumpTreeAndExit {
                eprintln!("\nCtrl+C pressed - Node tree dump:");
                match dump(&cnpy.core, cnpy.core.root, cnpy.core.focus) {
                    Ok(dump_str) => eprintln!("{dump_str}"),
                    Err(dump_err) => eprintln!("Failed to dump node tree: {dump_err}"),
                }
            }

            return Ok(130);
        }

        cnpy.event(event)?;
        cnpy.service_automation();
        if let Some(code) = cnpy.core.take_exit_request() {
            return Ok(code);
        }
        match cnpy.render_if_pending(&mut be) {
            Ok(rendered) => {
                if rendered && let Err(e) = translate_result(be.flush()) {
                    return Err(handle_render_error(
                        e,
                        &cnpy.core,
                        cnpy.core.root,
                        cnpy.core.focus,
                        &session,
                    ));
                }
            }
            Err(e) => {
                return Err(handle_render_error(
                    e,
                    &cnpy.core,
                    cnpy.core.root,
                    cnpy.core.focus,
                    &session,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll},
    };

    use futures::{channel::mpsc::unbounded, stream};

    use super::*;

    /// Pending stream that records when cancellation drops it.
    struct DropReader {
        dropped: Arc<AtomicBool>,
    }

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

    impl Write for FakeTerminal {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
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
    }

    impl Stream for DropReader {
        type Item = io::Result<cevent::Event>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    impl Drop for DropReader {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    fn text_run(x: u32, y: u32, text: &str) -> PositionedTextRun {
        PositionedTextRun {
            location: Point { x, y },
            text: text.to_string(),
        }
    }

    #[test]
    fn positioned_text_runs_split_after_wide_graphemes() {
        let runs = positioned_text_runs(Point { x: 5, y: 2 }, "a界bc");

        assert_eq!(
            runs,
            vec![
                text_run(5, 2, "a"),
                text_run(6, 2, "界"),
                text_run(8, 2, "bc"),
            ]
        );
    }

    #[test]
    fn terminal_capabilities_balance_in_reverse_order() -> io::Result<()> {
        let mut terminal = FakeTerminal::default();
        let mut capabilities = TerminalCapabilities::default();

        acquire_terminal(&mut terminal, &mut capabilities, true)?;
        release_terminal(&mut terminal, &mut capabilities)?;

        assert_eq!(
            terminal.calls,
            [
                "raw+",
                "screen+",
                "mouse+",
                "cursor-",
                "keyboard+",
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
        for fail_at in 1..=5 {
            let mut terminal = FakeTerminal {
                fail_at: Some(fail_at),
                ..FakeTerminal::default()
            };
            let mut capabilities = TerminalCapabilities::default();

            acquire_terminal(&mut terminal, &mut capabilities, true)
                .expect_err("configured acquisition should fail");
            release_terminal(&mut terminal, &mut capabilities)
                .expect("rollback should release every acquired capability");

            assert!(!capabilities.is_active(), "failed at step {fail_at}");
        }
    }

    #[test]
    fn event_source_surfaces_terminal_reader_failure() {
        let (_internal_tx, internal_rx) = unbounded();
        let terminal = stream::iter(vec![Err(io::Error::other("reader failed"))]);
        let mut events = EventSource::new(terminal, internal_rx);

        let error = block_on(events.next()).expect_err("reader failure should reach run loop");
        assert!(matches!(
            error,
            error::Error::TerminalIo(source) if source.to_string() == "reader failed"
        ));
    }

    #[test]
    fn dropping_event_source_cancels_terminal_reader() {
        let dropped = Arc::new(AtomicBool::new(false));
        let terminal = DropReader {
            dropped: Arc::clone(&dropped),
        };
        let (_internal_tx, internal_rx) = unbounded();

        drop(EventSource::new(terminal, internal_rx));

        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn internal_event_wakes_pending_terminal_reader() {
        let dropped = Arc::new(AtomicBool::new(false));
        let terminal = DropReader {
            dropped: Arc::clone(&dropped),
        };
        let (internal_tx, internal_rx) = unbounded();
        internal_tx
            .unbounded_send(Event::Wake)
            .expect("internal event receiver should be open");
        let mut events = EventSource::new(terminal, internal_rx);

        assert!(matches!(block_on(events.next()), Ok(Event::Wake)));
        drop(events);
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn positioned_text_runs_keep_combining_graphemes_in_run() {
        let runs = positioned_text_runs(Point { x: 1, y: 3 }, "e\u{0301}x");

        assert_eq!(runs, vec![text_run(1, 3, "e\u{0301}x")]);
    }
}
