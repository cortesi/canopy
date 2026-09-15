use std::{
    future::{Future, pending, poll_fn},
    io::{self, Stderr, Write},
    iter,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use futures::{channel::mpsc::UnboundedReceiver, pin_mut, stream::Stream};
use tokio::{
    runtime::Builder,
    time::{Instant as TokioInstant, sleep_until},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Canopy, Work,
    backend::{BackendControl, TerminalSession},
    core::{Core, canopy::AdapterEvent, dump::dump, text},
    error::{self, Result},
    event::{Event, key, mouse},
    geom::{Point, PointI32, Size},
    render::RenderBackend,
    style::{Color, ResolvedStyle},
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
/// Most ready input events one turn takes before it renders.
const INPUT_BATCH_LIMIT: usize = 256;

/// Simple event source wrapper for receiving events.
///
/// The run loop takes one event, then drains the input already waiting into
/// the same batch, so a burst of input costs one render.
struct EventSource<S> {
    /// Cancellable terminal event stream owned by the run loop.
    terminal: S,
    /// Framework event receiver channel.
    internal: UnboundedReceiver<AdapterEvent>,
    /// Framework wake that ended the last batch, returned by the next call.
    pending: Option<AdapterEvent>,
    /// Stream error met while draining, returned by the next call after its
    /// batch.
    deferred_error: Option<error::Error>,
    /// Alternate terminal and framework input when both remain ready.
    prefer_internal: bool,
}

impl<S> EventSource<S>
where
    S: Stream<Item = io::Result<cevent::Event>> + Unpin,
{
    /// Construct a new event source.
    fn new(terminal: S, internal: UnboundedReceiver<AdapterEvent>) -> Self {
        Self {
            terminal,
            internal,
            pending: None,
            deferred_error: None,
            prefer_internal: false,
        }
    }

    /// Poll one input source, preserving stream errors and termination.
    fn poll_source(
        &mut self,
        cx: &mut Context<'_>,
        internal: bool,
    ) -> Poll<Result<Option<AdapterEvent>>> {
        if internal {
            Pin::new(&mut self.internal).poll_next(cx).map(|event| {
                event
                    .map(Some)
                    .ok_or_else(|| error::Error::RunLoop("framework event channel closed".into()))
            })
        } else {
            Pin::new(&mut self.terminal)
                .poll_next(cx)
                .map(terminal_event)
                .map(|result| result.map(|event| event.map(AdapterEvent::Input)))
        }
    }

    /// Poll fairly while bounding ignored terminal events in one executor turn.
    fn poll_uncoalesced(&mut self, cx: &mut Context<'_>) -> Poll<Result<AdapterEvent>> {
        for _ in 0..64 {
            let first = self.prefer_internal;
            let (result, internal) = match self.poll_source(cx, first) {
                Poll::Pending => (self.poll_source(cx, !first), !first),
                ready => (ready, first),
            };
            match result {
                Poll::Ready(Ok(event)) => {
                    self.prefer_internal = !internal;
                    if let Some(event) = event {
                        return Poll::Ready(Ok(event));
                    }
                }
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    /// Await one event from either the terminal or framework channel.
    async fn next_uncoalesced(&mut self) -> Result<AdapterEvent> {
        poll_fn(|cx| self.poll_uncoalesced(cx)).await
    }

    /// Await the next event, returning a held stream error or framework wake
    /// first.
    async fn next(&mut self) -> Result<AdapterEvent> {
        if let Some(error) = self.deferred_error.take() {
            return Err(error);
        }
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }
        self.next_uncoalesced().await
    }

    /// Await the next unit of work: a framework wake, or a batch of input.
    ///
    /// A batch starts with the next input event and takes the input already
    /// waiting, at most [`INPUT_BATCH_LIMIT`] events, so a burst of input costs
    /// one render. Consecutive pointer moves, and consecutive drags with the
    /// same button and modifiers, collapse to the latest position.
    ///
    /// Draining polls with the caller's waker. A terminal stream such as
    /// crossterm's keeps the waker from its first pending poll to report the
    /// next event, so draining with a throwaway waker would leave the run
    /// loop asleep.
    async fn next_work(&mut self) -> Result<Work> {
        let AdapterEvent::Input(first) = self.next().await? else {
            return Ok(Work::Wake);
        };
        let mut batch = vec![first];
        poll_fn(|cx| {
            self.drain_ready(cx, &mut batch);
            Poll::Ready(())
        })
        .await;
        Ok(Work::Input(batch))
    }

    /// Move waiting input into `batch` until none is ready.
    ///
    /// A framework wake ends the batch and waits for the next call, and so does
    /// a stream error, so the input read before it is still delivered.
    fn drain_ready(&mut self, cx: &mut Context<'_>, batch: &mut Vec<Event>) {
        for _ in 0..INPUT_BATCH_LIMIT {
            let next = match self.poll_uncoalesced(cx) {
                Poll::Ready(Ok(event)) => event,
                Poll::Ready(Err(error)) => {
                    self.deferred_error = Some(error);
                    return;
                }
                Poll::Pending => return,
            };
            match next {
                AdapterEvent::Input(event) => push_coalesced(batch, event),
                AdapterEvent::Wake => {
                    self.pending = Some(AdapterEvent::Wake);
                    return;
                }
            }
        }
    }
}

/// Append `event` to `batch`, replacing the last event when both are the same
/// pointer motion.
fn push_coalesced(batch: &mut Vec<Event>, event: Event) {
    if let (Some(Event::Mouse(last)), Event::Mouse(next)) = (batch.last_mut(), &event)
        && same_motion(last, next)
    {
        *last = *next;
        return;
    }
    batch.push(event);
}

/// Return whether `next` continues the motion of `last`: a move after a move,
/// or a drag after a drag with the same button and modifiers.
fn same_motion(last: &mouse::MouseEvent, next: &mouse::MouseEvent) -> bool {
    matches!(
        (last.action, next.action),
        (mouse::Action::Moved, mouse::Action::Moved) | (mouse::Action::Drag, mouse::Action::Drag)
    ) && last.button == next.button
        && last.modifiers == next.modifiers
}

/// Select input, runtime notification, or a timer with rotating ready priority.
async fn select_work<E, W, D>(
    event: E,
    wake: W,
    deadline: D,
    next_source: &mut usize,
) -> Result<Work>
where
    E: Future<Output = Result<Work>>,
    W: Future<Output = Result<()>>,
    D: Future<Output = ()>,
{
    pin_mut!(event, wake, deadline);
    poll_fn(|cx| {
        for offset in 0..3 {
            let source = (*next_source + offset) % 3;
            let ready = match source {
                0 => event.as_mut().poll(cx),
                1 => wake.as_mut().poll(cx).map(|wake| wake.map(|()| Work::Wake)),
                _ => deadline.as_mut().poll(cx).map(|()| Ok(Work::Wake)),
            };
            if ready.is_ready() {
                *next_source = (source + 1) % 3;
                return ready;
            }
        }
        Poll::Pending
    })
    .await
}

/// Wait without holding any widget or VM borrow across suspension.
async fn next_runtime_work<S>(
    events: &mut EventSource<S>,
    canopy: &Canopy,
    deadline: Option<Instant>,
    next_source: &mut usize,
) -> Result<Work>
where
    S: Stream<Item = io::Result<cevent::Event>> + Unpin,
{
    let timer = async move {
        match deadline {
            Some(deadline) => sleep_until(TokioInstant::from_std(deadline)).await,
            None => pending::<()>().await,
        }
    };
    select_work(
        events.next_work(),
        poll_fn(|cx| canopy.poll_runtime_wake(cx)),
        timer,
        next_source,
    )
    .await
}

/// Translate one terminal stream item or report reader termination.
fn terminal_event(event: Option<io::Result<cevent::Event>>) -> Result<Option<Event>> {
    match event {
        Some(Ok(cevent::Event::Key(event)))
            if event.kind == cevent::KeyEventKind::Release
                || matches!(
                    event.code,
                    cevent::KeyCode::Media(_) | cevent::KeyCode::Modifier(_)
                ) =>
        {
            Ok(None)
        }
        Some(Ok(event)) => Ok(Some(translate_event(event))),
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
    e.map_err(error::Error::TerminalIo)
}

/// Convert a terminal cell coordinate into the crossterm `u16` range.
fn cell_coord(value: u32) -> io::Result<u16> {
    u16::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal coordinate exceeds u16",
        )
    })
}

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

/// Crossterm-backed render backend.
pub struct CrosstermRender {
    /// Stderr handle used for rendering output.
    fp: Stderr,
    /// Encoded commands for the current frame, discarded if emission fails.
    pending: Vec<u8>,
}

impl CrosstermRender {
    /// Flush pending output.
    fn flush(&mut self) -> io::Result<()> {
        flush_frame(&mut self.fp.lock(), &mut self.pending)
    }

    /// Apply a style to subsequent output.
    fn apply_style(&mut self, s: &ResolvedStyle) -> io::Result<()> {
        // Always reset first to clear any previous attributes, then set colors
        // and attrs. Order is important: reset clears everything, so we
        // must set colors after.
        self.pending
            .queue(style::SetAttribute(style::Attribute::Reset))?;
        self.pending
            .queue(style::SetForegroundColor(translate_color(s.fg)))?;
        self.pending
            .queue(style::SetBackgroundColor(translate_color(s.bg)))?;

        // Now add the desired attributes
        if s.attrs.bold {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Bold))?;
        }
        if s.attrs.crossedout {
            self.pending
                .queue(style::SetAttribute(style::Attribute::CrossedOut))?;
        }
        if s.attrs.dim {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Dim))?;
        }
        if s.attrs.italic {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Italic))?;
        }
        if s.attrs.overline {
            self.pending
                .queue(style::SetAttribute(style::Attribute::OverLined))?;
        }
        if s.attrs.underline {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Underlined))?;
        }
        Ok(())
    }

    /// Write text at a position.
    fn text(&mut self, loc: Point, txt: &str) -> io::Result<()> {
        for run in positioned_text_runs(loc, txt) {
            let x = cell_coord(run.location.x)?;
            let y = cell_coord(run.location.y)?;
            self.pending.queue(ccursor::MoveTo(x, y))?;
            self.pending.queue(style::Print(run.text))?;
        }
        Ok(())
    }
}

/// Write one frame and discard its bytes even on a partial write or flush
/// error. The caller can then encode a full repaint for the next attempt.
fn flush_frame(writer: &mut impl Write, pending: &mut Vec<u8>) -> io::Result<()> {
    let result = writer.write_all(pending);
    pending.clear();
    result?;
    writer.flush()
}

/// A string fragment with an absolute terminal-cell location.
#[derive(Debug, PartialEq, Eq)]
struct PositionedTextRun<'a> {
    /// Location where the text run should be printed.
    location: Point,
    /// Text to print at the location.
    text: &'a str,
}

/// Borrow absolute-positioned runs, isolating wide graphemes and omitting
/// zero-width graphemes without allocating intermediate strings.
fn positioned_text_runs(loc: Point, txt: &str) -> impl Iterator<Item = PositionedTextRun<'_>> {
    let mut graphemes = txt.grapheme_indices(true).peekable();
    let mut x = loc.x;

    iter::from_fn(move || {
        loop {
            let (start, grapheme) = graphemes.next()?;
            let width = text::grapheme_width(grapheme);
            if width == 0 {
                continue;
            }

            let location = Point { x, y: loc.y };
            let mut end = start + grapheme.len();
            x = x.saturating_add(width as u32);
            if width == 1 {
                while let Some((offset, following)) =
                    graphemes.next_if(|(_, next)| text::grapheme_width(next) == 1)
                {
                    end = offset + following.len();
                    x = x.saturating_add(1);
                }
            }
            return Some(PositionedTextRun {
                location,
                text: &txt[start..end],
            });
        }
    })
}

impl Default for CrosstermRender {
    fn default() -> Self {
        Self {
            fp: io::stderr(),
            pending: Vec::with_capacity(64 * 1024),
        }
    }
}

impl RenderBackend for CrosstermRender {
    fn reset(&mut self) -> Result<()> {
        self.pending.clear();
        Ok(())
    }

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
        let x = translate_result(cell_coord(loc.x))?;
        let y = translate_result(cell_coord(loc.y))?;
        translate_result(self.pending.queue(ccursor::MoveTo(x, y)))?;
        let seq = if count > 0 {
            format!("\x1b[{count_abs}@")
        } else {
            format!("\x1b[{count_abs}P")
        };
        translate_result(self.pending.queue(style::Print(seq)))?;
        Ok(())
    }

    fn shift_lines(&mut self, top: u32, bottom: u32, count: i32) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let top = top.min(u16::MAX as u32) as u16;
        let bottom = bottom.min(u16::MAX as u32) as u16;
        if top > bottom {
            return Ok(());
        }
        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        let region = format!("\x1b[{};{}r", top + 1, bottom + 1);
        translate_result(self.pending.queue(style::Print(region)))?;
        translate_result(self.pending.queue(ccursor::MoveTo(0, top)))?;
        let seq = if count > 0 {
            format!("\x1b[{count_abs}T")
        } else {
            format!("\x1b[{count_abs}S")
        };
        translate_result(self.pending.queue(style::Print(seq)))?;
        translate_result(self.pending.queue(style::Print("\x1b[r")))?;
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
                cevent::KeyCode::Media(_) | cevent::KeyCode::Modifier(_) => {
                    unreachable!("media and modifier keys are filtered before translation")
                }
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
                location: PointI32 {
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

/// Run the main render/event loop using the crossterm backend.
///
/// Ctrl+C dumps the node tree and stops the loop with status 130. Keyboard
/// enhancement flags are enabled so escape codes are unambiguous.
pub fn runloop(cnpy: Canopy) -> Result<i32> {
    runloop_with_options(cnpy, RunOptions::default())
}

/// Run the terminal adapter with explicit interrupt and emergency-key policies.
pub fn runloop_with_options(mut cnpy: Canopy, options: RunOptions) -> Result<i32> {
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
    let prepared = cnpy.turn(Work::Prepare)?;
    cnpy.emit_frame(&mut be)
        .map_err(|error| handle_render_error(error, &cnpy.core, &mut session))?;
    if let Some(code) = prepared.exit_code {
        return Ok(code);
    }

    let mut next_source = 0;
    loop {
        let deadline = cnpy.next_deadline();
        let work = runtime.block_on(next_runtime_work(
            &mut events,
            &cnpy,
            deadline,
            &mut next_source,
        ))?;

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
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        future::ready,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc,
        },
        task::{Context, Poll, Waker},
        thread,
        time::Duration,
    };

    use futures::{StreamExt, channel::mpsc::unbounded, executor::block_on, stream};

    use super::*;
    use crate::{
        EvalRequest,
        testing::{backend::TestRender, contracts},
    };

    #[derive(Default)]
    struct FrameCapture {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
        fail_after: Option<usize>,
        fail_flush: bool,
    }

    impl Write for FrameCapture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            let len = self.fail_after.map_or(bytes.len(), |limit| {
                limit.saturating_sub(self.bytes.len()).min(bytes.len())
            });
            if len == 0 && !bytes.is_empty() {
                return Err(io::Error::other("injected frame write failure"));
            }
            self.bytes.extend_from_slice(&bytes[..len]);
            Ok(len)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(io::Error::other("injected frame flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn terminal_frame_batches_text_and_shifts_until_flush() -> Result<()> {
        let mut backend = CrosstermRender::default();
        backend.text(Point { x: 1, y: 2 }, "hi").unwrap();
        backend.shift_chars(Point { x: 3, y: 2 }, 2)?;
        backend.shift_lines(2, 4, -1)?;
        let expected = b"\x1b[3;2Hhi\x1b[3;4H\x1b[2@\x1b[3;5r\x1b[3;1H\x1b[1S\x1b[r";
        assert_eq!(backend.pending, expected);

        let mut capture = FrameCapture::default();
        flush_frame(&mut capture, &mut backend.pending).unwrap();
        assert_eq!(capture.bytes, expected);
        assert_eq!((capture.writes, capture.flushes), (1, 1));
        assert!(backend.pending.is_empty());

        backend.text(Point::default(), "discarded").unwrap();
        backend.reset()?;
        assert!(backend.pending.is_empty());
        Ok(())
    }

    #[test]
    fn terminal_frame_failure_discards_bytes_before_a_fresh_retry() {
        for fail_after in [None, Some(3)] {
            let mut capture = FrameCapture {
                fail_after,
                fail_flush: fail_after.is_none(),
                ..FrameCapture::default()
            };
            let mut pending = b"first frame".to_vec();
            assert!(flush_frame(&mut capture, &mut pending).is_err());
            assert!(
                pending.is_empty(),
                "failed output must not survive for drop or retry"
            );
            assert_eq!(
                capture.bytes,
                if fail_after.is_some() {
                    &b"fir"[..]
                } else {
                    &b"first frame"[..]
                }
            );

            capture.bytes.clear();
            capture.fail_after = None;
            capture.fail_flush = false;
            pending.extend_from_slice(b"fresh frame");
            flush_frame(&mut capture, &mut pending).unwrap();
            assert_eq!(capture.bytes, b"fresh frame");
        }
    }

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
        let mut next_source = 0;
        let work = block_on(next_runtime_work(
            &mut events,
            &canopy,
            None,
            &mut next_source,
        ))?;
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

    fn text_run(x: u32, y: u32, text: &str) -> PositionedTextRun<'_> {
        PositionedTextRun {
            location: Point { x, y },
            text,
        }
    }

    #[test]
    fn positioned_text_runs_split_after_wide_graphemes() {
        let runs: Vec<_> = positioned_text_runs(Point { x: 5, y: 2 }, "a界bc").collect();

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

            acquire_terminal(&mut terminal, &mut capabilities)
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
            .unbounded_send(AdapterEvent::Wake)
            .expect("internal event receiver should be open");
        let mut events = EventSource::new(terminal, internal_rx);

        assert!(matches!(block_on(events.next()), Ok(AdapterEvent::Wake)));
        drop(events);
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn positioned_text_runs_keep_combining_graphemes_in_run() {
        let runs: Vec<_> = positioned_text_runs(Point { x: 1, y: 3 }, "e\u{0301}x").collect();

        assert_eq!(runs, vec![text_run(1, 3, "e\u{0301}x")]);
    }

    #[test]
    fn positioned_text_runs_skip_zero_width_without_losing_columns() {
        let runs: Vec<_> = positioned_text_runs(
            Point { x: 0, y: 2 },
            "\u{200b}a\u{200b}b\u{0301}界\u{200b}x",
        )
        .collect();
        assert_eq!(
            runs,
            [
                text_run(0, 2, "a"),
                text_run(1, 2, "b\u{0301}"),
                text_run(2, 2, "界"),
                text_run(4, 2, "x"),
            ]
        );
        assert!(
            positioned_text_runs(Point::default(), "\u{200b}")
                .next()
                .is_none()
        );
    }

    #[test]
    fn positioned_text_runs_saturate_coordinates() {
        let runs: Vec<_> = positioned_text_runs(
            Point {
                x: u32::MAX - 1,
                y: 0,
            },
            "界x",
        )
        .collect();
        assert_eq!(
            runs,
            [text_run(u32::MAX - 1, 0, "界"), text_run(u32::MAX, 0, "x")]
        );
    }
    fn terminal_key(kind: cevent::KeyEventKind) -> cevent::Event {
        cevent::Event::Key(cevent::KeyEvent::new_with_kind(
            cevent::KeyCode::Char('a'),
            cevent::KeyModifiers::empty(),
            kind,
        ))
    }

    fn terminal_drag(column: u16) -> cevent::Event {
        cevent::Event::Mouse(cevent::MouseEvent {
            kind: cevent::MouseEventKind::Drag(cevent::MouseButton::Left),
            column,
            row: 0,
            modifiers: cevent::KeyModifiers::empty(),
        })
    }

    fn terminal_move(column: u16) -> cevent::Event {
        cevent::Event::Mouse(cevent::MouseEvent {
            kind: cevent::MouseEventKind::Moved,
            column,
            row: 0,
            modifiers: cevent::KeyModifiers::empty(),
        })
    }

    #[test]
    fn event_source_ignores_releases_and_preserves_repeats() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Ok(terminal_key(cevent::KeyEventKind::Repeat)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
        ]);
        let mut events = EventSource::new(terminal, rx);
        for _ in 0..2 {
            assert!(matches!(
                block_on(events.next())?,
                AdapterEvent::Input(Event::Key(key::Key {
                    key: key::KeyCode::Char('a'),
                    ..
                }))
            ));
        }
        assert!(matches!(
            block_on(events.next()),
            Err(error::Error::RunLoop(_))
        ));
        Ok(())
    }

    #[test]
    fn event_source_errors_surface_directly_and_after_a_batch() -> Result<()> {
        // Awaiting the next event returns the error at once.
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Err(io::Error::other("after release")),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(
            matches!(block_on(events.next()), Err(error::Error::TerminalIo(error)) if error.to_string() == "after release")
        );

        // An error met while draining keeps the batch and returns next.
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Err(io::Error::other("after release")),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(block_on(events.next_work())?, Work::Input(batch) if batch.len() == 1));
        assert!(
            matches!(block_on(events.next_work()), Err(error::Error::TerminalIo(error)) if error.to_string() == "after release")
        );
        Ok(())
    }

    #[test]
    fn event_source_returns_eof_after_the_batch_before_it() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
        ]);
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(block_on(events.next_work())?, Work::Input(batch) if batch.len() == 1));
        assert!(matches!(
            block_on(events.next_work()),
            Err(error::Error::RunLoop(_))
        ));
        Ok(())
    }

    /// Return each batched event as a short label.
    fn batch_summary(batch: &[Event]) -> Vec<String> {
        batch
            .iter()
            .map(|event| match event {
                Event::Mouse(mouse) => format!("{:?}@{}", mouse.action, mouse.location.x),
                Event::Key(_) => "key".to_string(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn ready_input_batches_and_coalesces_moves_and_drags() -> Result<()> {
        let (_tx, rx) = unbounded();
        let terminal = stream::iter([
            Ok(terminal_move(1)),
            Ok(terminal_key(cevent::KeyEventKind::Release)),
            Ok(terminal_move(2)),
            Ok(terminal_drag(3)),
            Ok(terminal_drag(4)),
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_drag(5)),
        ])
        .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        let Work::Input(batch) = block_on(events.next_work())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&batch),
            ["Moved@2", "Drag@4", "key", "Drag@5"]
        );
        Ok(())
    }

    #[test]
    fn a_ready_input_batch_takes_a_bounded_number_of_events() -> Result<()> {
        let (_tx, rx) = unbounded();
        let drags = (0..INPUT_BATCH_LIMIT as u16 + 10)
            .map(|column| Ok(terminal_drag(column)))
            .collect::<Vec<_>>();
        let terminal = stream::iter(drags).chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        let Work::Input(first) = block_on(events.next_work())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&first),
            [format!("Drag@{INPUT_BATCH_LIMIT}")],
            "the drags collapse to the last one taken"
        );
        let Work::Input(rest) = block_on(events.next_work())? else {
            panic!("the terminal produced input");
        };
        assert_eq!(
            batch_summary(&rest),
            [format!("Drag@{}", INPUT_BATCH_LIMIT + 9)],
            "input past the limit waits for the next batch"
        );
        Ok(())
    }

    /// A terminal stream that, like crossterm's, keeps only the waker from its
    /// first pending poll until an event arrives.
    struct OneWakerStream(Arc<Mutex<(VecDeque<cevent::Event>, Option<Waker>)>>);

    impl Stream for OneWakerStream {
        type Item = io::Result<cevent::Event>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let mut state = self.0.lock().expect("stream state");
            if let Some(event) = state.0.pop_front() {
                return Poll::Ready(Some(Ok(event)));
            }
            if state.1.is_none() {
                state.1 = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }

    #[test]
    fn input_after_a_drain_wakes_the_loop() {
        let (done_tx, done_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let state = Arc::new(Mutex::new((
                VecDeque::from([terminal_key(cevent::KeyEventKind::Press)]),
                None,
            )));
            let (_tx, rx) = unbounded();
            let mut events = EventSource::new(OneWakerStream(state.clone()), rx);
            // Draining after the first key finds nothing and leaves its waker
            // with the stream, as crossterm does.
            let first = block_on(events.next_work());
            let pusher = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                let mut guard = state.lock().expect("stream state");
                guard.0.push_back(terminal_key(cevent::KeyEventKind::Press));
                if let Some(waker) = guard.1.take() {
                    waker.wake();
                }
            });
            let second = block_on(events.next_work());
            pusher.join().expect("pusher thread");
            let _sent = done_tx
                .send(matches!(first, Ok(Work::Input(_))) && matches!(second, Ok(Work::Input(_))));
        });
        // A regression parks the worker forever, so wait with a timeout and
        // join only a worker that finished.
        let woke = done_rx.recv_timeout(Duration::from_secs(5));
        if woke.is_ok() {
            worker.join().expect("worker thread");
        }
        assert_eq!(
            woke,
            Ok(true),
            "input that arrives after a drain must wake the loop"
        );
    }

    #[test]
    fn event_source_release_does_not_consume_framework_wake() -> Result<()> {
        let (tx, rx) = unbounded();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        let terminal = stream::iter([Ok(terminal_key(cevent::KeyEventKind::Release))])
            .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        Ok(())
    }

    #[test]
    fn adapter_rotates_between_ready_input_wake_and_deadline() -> Result<()> {
        let mut next_source = 0;
        for expected_source in 0..3 {
            let work = block_on(select_work(
                ready(Ok(Work::Input(vec![Event::FocusGained]))),
                ready(Ok(())),
                ready(()),
                &mut next_source,
            ))?;
            assert_eq!(next_source, (expected_source + 1) % 3);
            if expected_source == 0 {
                assert!(
                    matches!(&work, Work::Input(events) if matches!(events.as_slice(), [Event::FocusGained]))
                );
            } else {
                assert!(matches!(work, Work::Wake));
            }
        }
        Ok(())
    }

    #[test]
    fn adapter_services_deadlines_without_terminal_input() -> Result<()> {
        let mut next_source = 0;
        let work = block_on(select_work(
            pending::<Result<Work>>(),
            pending::<Result<()>>(),
            ready(()),
            &mut next_source,
        ))?;
        assert!(matches!(work, Work::Wake));
        Ok(())
    }

    #[test]
    fn event_source_alternates_ready_terminal_and_internal_events() -> Result<()> {
        let (tx, rx) = unbounded();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        tx.unbounded_send(AdapterEvent::Wake).unwrap();
        let terminal = stream::iter([
            Ok(terminal_key(cevent::KeyEventKind::Press)),
            Ok(terminal_key(cevent::KeyEventKind::Press)),
        ])
        .chain(stream::pending());
        let mut events = EventSource::new(terminal, rx);
        assert!(matches!(
            block_on(events.next())?,
            AdapterEvent::Input(Event::Key(_))
        ));
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        assert!(matches!(
            block_on(events.next())?,
            AdapterEvent::Input(Event::Key(_))
        ));
        assert!(matches!(block_on(events.next())?, AdapterEvent::Wake));
        Ok(())
    }
}
