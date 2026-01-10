use std::{
    io::{self, Stderr, Write},
    panic,
    result::Result as StdResult,
    sync::mpsc::{self, TryRecvError},
    thread,
};

use color_backtrace::{BacktracePrinter, default_output_stream};
use scopeguard::guard;

use crate::{
    Canopy, NodeId,
    backend::{BackendControl, TerminalSession},
    core::{
        Core,
        dump::{dump, dump_with_focus},
    },
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Expanse, Point},
    render::RenderBackend,
    style::{Color, Style},
};
/// Simple event source wrapper for receiving events.
///
/// This coalesces consecutive mouse-move events so clicks are not delayed by move bursts.
struct EventSource {
    /// Event receiver channel.
    rx: mpsc::Receiver<Event>,
    /// Buffered non-move event encountered while coalescing.
    pending: Option<Event>,
}

impl EventSource {
    /// Construct a new event source.
    fn new(rx: mpsc::Receiver<Event>) -> Self {
        Self { rx, pending: None }
    }

    /// Block until the next event arrives.
    fn next(&mut self) -> StdResult<Event, mpsc::RecvError> {
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }

        let mut event = self.rx.recv()?;
        if matches!(
            event,
            Event::Mouse(mouse::MouseEvent {
                action: mouse::Action::Moved,
                ..
            })
        ) {
            loop {
                match self.rx.try_recv() {
                    Ok(next) => {
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
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
        }

        Ok(event)
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
        Err(e) => Err(error::Error::Render(e.to_string())),
    }
}

/// Crossterm-backed implementation of `BackendControl`.
#[derive(Debug)]
pub struct CrosstermControl {
    /// Stderr handle used for control output.
    fp: Stderr,
}

impl CrosstermControl {
    /// Enter alternate screen and raw mode.
    fn enter(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.fp.execute(terminal::EnterAlternateScreen)?;
        self.fp.execute(cevent::EnableMouseCapture)?;
        self.fp.execute(ccursor::Hide)?;
        Ok(())
    }
    /// Leave alternate screen and restore terminal state.
    fn exit(&mut self) -> io::Result<()> {
        self.fp.execute(terminal::LeaveAlternateScreen)?;
        self.fp.execute(cevent::DisableMouseCapture)?;
        self.fp.execute(ccursor::Show)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Default for CrosstermControl {
    fn default() -> Self {
        Self { fp: io::stderr() }
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
    fn apply_style(&mut self, s: &Style) -> io::Result<()> {
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
        self.fp.queue(ccursor::MoveTo(loc.x as u16, loc.y as u16))?;
        self.fp.queue(style::Print(txt))?;
        Ok(())
    }
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

    fn style(&mut self, s: &Style) -> Result<()> {
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
        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        if count > 0 {
            translate_result(self.fp.queue(terminal::ScrollDown(count_abs)))?;
        } else {
            translate_result(self.fp.queue(terminal::ScrollUp(count_abs)))?;
        }
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
        cevent::Event::Resize(x, y) => Event::Resize(Expanse::new(x.into(), y.into())),
        cevent::Event::FocusGained => Event::FocusGained,
        cevent::Event::FocusLost => Event::FocusLost,
        cevent::Event::Paste(s) => Event::Paste(s),
    }
}

/// Thread entry that forwards crossterm events into the channel.
fn event_emitter(evt_tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        loop {
            match cevent::read() {
                Ok(evt) => {
                    if evt_tx.send(translate_event(evt)).is_err() {
                        // The receiver has been dropped, which usually means the application is shutting down.
                        return;
                    }
                }
                Err(e) => {
                    // Log the error and notify the main loop if possible, or exit gracefully.
                    tracing::error!("Crossterm event read error: {}", e);
                    // We can't easily notify the main loop without a dedicated error channel or
                    // a special Event variant. For now, we'll just exit the thread.
                    return;
                }
            }
        }
    });
}

/// Helper function to handle render errors by exiting alternate screen mode
/// and displaying the error with a node tree dump
fn handle_render_error(
    error: error::Error,
    core: &Core,
    root: NodeId,
    focus: Option<NodeId>,
    session: &mut TerminalSession,
) -> error::Error {
    drop(session.stop());

    // Print error and node dump
    eprintln!("Render error: {error}");
    eprintln!("\nNode tree dump:");
    let dump_result = if focus.is_some() {
        dump_with_focus(core, root, focus)
    } else {
        dump(core, root)
    };
    match dump_result {
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
    cnpy.register_backend(CrosstermControl::default());
    let mut session = {
        let backend = cnpy
            .core
            .backend
            .as_mut()
            .ok_or_else(|| error::Error::Internal("backend not set".into()))?;
        TerminalSession::new(backend)?
    };

    let _panic_hook = if options.install_panic_hook {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(|pi| {
            let mut stderr = io::stderr();
            #[allow(unused_must_use)]
            {
                crossterm::execute!(
                    stderr,
                    terminal::LeaveAlternateScreen,
                    cevent::DisableMouseCapture,
                    ccursor::Show
                );
                terminal::disable_raw_mode();
                BacktracePrinter::new().print_panic_info(pi, &mut default_output_stream());
            }
        }));
        Some(guard(previous, |hook| {
            panic::set_hook(hook);
        }))
    } else {
        None
    };

    let rx = if let Some(x) = cnpy.event_rx.take() {
        x
    } else {
        panic!("core event loop already initialized")
    };

    let mut events = EventSource::new(rx);
    event_emitter(cnpy.event_tx.clone());
    let size = translate_result(terminal::size())?;
    cnpy.set_root_size(Expanse::new(size.0.into(), size.1.into()))?;
    cnpy.start_poller(cnpy.event_tx.clone());

    if let Err(e) = cnpy.render(&mut be) {
        return Err(handle_render_error(
            e,
            &cnpy.core,
            cnpy.core.root,
            cnpy.core.focus,
            &mut session,
        ));
    }
    translate_result(be.flush())?;
    if let Some(code) = cnpy.core.take_exit_request() {
        return Ok(code);
    }

    loop {
        let event = events.next()?;

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
                match dump_with_focus(&cnpy.core, cnpy.core.root, cnpy.core.focus) {
                    Ok(dump_str) => eprintln!("{dump_str}"),
                    Err(dump_err) => eprintln!("Failed to dump node tree: {dump_err}"),
                }
            }

            return Ok(130);
        }

        cnpy.event(event)?;
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
                        &mut session,
                    ));
                }
            }
            Err(e) => {
                return Err(handle_render_error(
                    e,
                    &cnpy.core,
                    cnpy.core.root,
                    cnpy.core.focus,
                    &mut session,
                ));
            }
        }
    }
}
