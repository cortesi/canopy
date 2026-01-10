use std::{
    ffi::OsString,
    io::{Read, Write},
    mem,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use alacritty_terminal::{
    event as alacritty_event,
    event::EventListener,
    grid::{Dimensions, Scroll},
    index,
    selection::{Selection, SelectionType},
    term::{self, Config, Term, TermMode},
    vte::ansi,
};
use canopy::{
    Context, EventOutcome, ReadContext, Widget, cursor, derive_commands,
    error::{Error, Result},
    event::{self, key, mouse},
    geom,
    layout::{CanvasContext, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, Color, Style},
};
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};

/// Fallback terminal column count before sizing is known.
const DEFAULT_COLUMNS: usize = 80;
/// Fallback terminal line count before sizing is known.
const DEFAULT_LINES: usize = 24;
/// Default scrollback history length.
const DEFAULT_SCROLLBACK: usize = 10_000;
/// Poll interval for draining PTY output.
const POLL_INTERVAL_MS: u64 = 16;
/// Maximum delay between clicks to count as a multi-click selection.
const DOUBLE_CLICK_MS: u64 = 400;
/// Lines to scroll per mouse wheel tick when not reporting mouse.
const SCROLL_LINES: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Terminal grid sizing and scrollback metadata.
struct TerminalSize {
    /// Visible terminal columns.
    columns: usize,
    /// Visible terminal rows.
    screen_lines: usize,
    /// Scrollback buffer length.
    scrollback_lines: usize,
}

impl TerminalSize {
    /// Construct a terminal size ensuring non-zero dimensions.
    fn new(columns: usize, screen_lines: usize, scrollback_lines: usize) -> Self {
        Self {
            columns: columns.max(1),
            screen_lines: screen_lines.max(1),
            scrollback_lines,
        }
    }

    /// Convert a Canopy expanse into a terminal grid size.
    fn from_expanse(expanse: geom::Expanse, scrollback_lines: usize) -> Self {
        Self::new(expanse.w as usize, expanse.h as usize, scrollback_lines)
    }

    /// Render the PTY size payload for portable-pty.
    fn pty_size(self) -> PtySize {
        PtySize {
            rows: self.screen_lines.min(u16::MAX as usize) as u16,
            cols: self.columns.min(u16::MAX as usize) as u16,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    /// Render the terminal window size payload for alacritty.
    fn window_size(self) -> alacritty_event::WindowSize {
        alacritty_event::WindowSize {
            num_lines: self.screen_lines.min(u16::MAX as usize) as u16,
            num_cols: self.columns.min(u16::MAX as usize) as u16,
            cell_width: 0,
            cell_height: 0,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines.saturating_add(self.scrollback_lines)
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Simple clipboard store used by alacritty for selection and paste callbacks.
#[derive(Default)]
struct TerminalClipboard {
    /// Stored clipboard contents.
    clipboard: String,
    /// Stored selection contents.
    selection: String,
}

impl TerminalClipboard {
    /// Store clipboard or selection contents.
    fn store(&mut self, ty: term::ClipboardType, text: String) {
        match ty {
            term::ClipboardType::Clipboard => self.clipboard = text,
            term::ClipboardType::Selection => self.selection = text,
        }
    }

    /// Load clipboard or selection contents.
    fn load(&self, ty: term::ClipboardType) -> &str {
        match ty {
            term::ClipboardType::Clipboard => &self.clipboard,
            term::ClipboardType::Selection => &self.selection,
        }
    }
}

/// Event bridge used by alacritty to forward terminal events back to the widget.
#[derive(Clone)]
struct EventProxy {
    /// Shared queue of alacritty events.
    events: Arc<Mutex<Vec<alacritty_event::Event>>>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: alacritty_event::Event) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

/// Track click timing for selection behavior.
struct ClickState {
    /// Last click location.
    location: geom::Point,
    /// Last click timestamp.
    last_click: Instant,
    /// Number of clicks in the current multi-click sequence.
    count: u8,
}

/// Terminal color palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalColors {
    /// ANSI black (0).
    pub black: Color,
    /// ANSI red (1).
    pub red: Color,
    /// ANSI green (2).
    pub green: Color,
    /// ANSI yellow (3).
    pub yellow: Color,
    /// ANSI blue (4).
    pub blue: Color,
    /// ANSI magenta (5).
    pub magenta: Color,
    /// ANSI cyan (6).
    pub cyan: Color,
    /// ANSI white (7).
    pub white: Color,
    /// ANSI bright black (8).
    pub bright_black: Color,
    /// ANSI bright red (9).
    pub bright_red: Color,
    /// ANSI bright green (10).
    pub bright_green: Color,
    /// ANSI bright yellow (11).
    pub bright_yellow: Color,
    /// ANSI bright blue (12).
    pub bright_blue: Color,
    /// ANSI bright magenta (13).
    pub bright_magenta: Color,
    /// ANSI bright cyan (14).
    pub bright_cyan: Color,
    /// ANSI bright white (15).
    pub bright_white: Color,
    /// Default foreground color.
    pub foreground: Color,
    /// Default background color.
    pub background: Color,
    /// Cursor color.
    pub cursor: Color,
}

impl Default for TerminalColors {
    fn default() -> Self {
        Self {
            black: Color::rgb("#000000"),
            red: Color::rgb("#cc0000"),
            green: Color::rgb("#4e9a06"),
            yellow: Color::rgb("#c4a000"),
            blue: Color::rgb("#3465a4"),
            magenta: Color::rgb("#75507b"),
            cyan: Color::rgb("#06989a"),
            white: Color::rgb("#d3d7cf"),
            bright_black: Color::rgb("#555753"),
            bright_red: Color::rgb("#ef2929"),
            bright_green: Color::rgb("#8ae234"),
            bright_yellow: Color::rgb("#fce94f"),
            bright_blue: Color::rgb("#729fcf"),
            bright_magenta: Color::rgb("#ad7fa8"),
            bright_cyan: Color::rgb("#34e2e2"),
            bright_white: Color::rgb("#eeeeec"),
            foreground: Color::rgb("#eeeeec"),
            background: Color::rgb("#000000"),
            cursor: Color::rgb("#ffffff"),
        }
    }
}

impl TerminalColors {
    /// Resolve an alacritty color using overrides and the configured palette.
    fn resolve_color(&self, color: ansi::Color, overrides: &term::color::Colors) -> Color {
        match color {
            ansi::Color::Spec(rgb) => Self::rgb_color(rgb),
            ansi::Color::Indexed(idx) => self.resolve_indexed(idx, overrides),
            ansi::Color::Named(named) => {
                if let Some(rgb) = overrides[named] {
                    Self::rgb_color(rgb)
                } else {
                    self.resolve_named(named)
                }
            }
        }
    }

    /// Resolve a color request for OSC color queries.
    fn resolve_request_color(&self, index: usize, overrides: &term::color::Colors) -> Color {
        if index < term::color::COUNT
            && let Some(rgb) = overrides[index]
        {
            return Self::rgb_color(rgb);
        }

        if index < 256 {
            return self.resolve_indexed(index as u8, overrides);
        }

        match index {
            256 => self.foreground,
            257 => self.background,
            258 => self.cursor,
            259 => self.dim_color(self.black),
            260 => self.dim_color(self.red),
            261 => self.dim_color(self.green),
            262 => self.dim_color(self.yellow),
            263 => self.dim_color(self.blue),
            264 => self.dim_color(self.magenta),
            265 => self.dim_color(self.cyan),
            266 => self.dim_color(self.white),
            267 => self.bright_color(self.foreground),
            268 => self.dim_color(self.foreground),
            _ => self.foreground,
        }
    }

    /// Resolve indexed palette colors, respecting overrides.
    fn resolve_indexed(&self, idx: u8, overrides: &term::color::Colors) -> Color {
        let index = idx as usize;
        if index < term::color::COUNT
            && let Some(rgb) = overrides[index]
        {
            return Self::rgb_color(rgb);
        }

        match idx {
            0 => self.black,
            1 => self.red,
            2 => self.green,
            3 => self.yellow,
            4 => self.blue,
            5 => self.magenta,
            6 => self.cyan,
            7 => self.white,
            8 => self.bright_black,
            9 => self.bright_red,
            10 => self.bright_green,
            11 => self.bright_yellow,
            12 => self.bright_blue,
            13 => self.bright_magenta,
            14 => self.bright_cyan,
            15 => self.bright_white,
            _ => Color::AnsiValue(idx),
        }
    }

    /// Resolve named palette colors.
    fn resolve_named(&self, named: ansi::NamedColor) -> Color {
        match named {
            ansi::NamedColor::Black => self.black,
            ansi::NamedColor::Red => self.red,
            ansi::NamedColor::Green => self.green,
            ansi::NamedColor::Yellow => self.yellow,
            ansi::NamedColor::Blue => self.blue,
            ansi::NamedColor::Magenta => self.magenta,
            ansi::NamedColor::Cyan => self.cyan,
            ansi::NamedColor::White => self.white,
            ansi::NamedColor::BrightBlack => self.bright_black,
            ansi::NamedColor::BrightRed => self.bright_red,
            ansi::NamedColor::BrightGreen => self.bright_green,
            ansi::NamedColor::BrightYellow => self.bright_yellow,
            ansi::NamedColor::BrightBlue => self.bright_blue,
            ansi::NamedColor::BrightMagenta => self.bright_magenta,
            ansi::NamedColor::BrightCyan => self.bright_cyan,
            ansi::NamedColor::BrightWhite => self.bright_white,
            ansi::NamedColor::Foreground => self.foreground,
            ansi::NamedColor::Background => self.background,
            ansi::NamedColor::Cursor => self.cursor,
            ansi::NamedColor::DimBlack => self.dim_color(self.black),
            ansi::NamedColor::DimRed => self.dim_color(self.red),
            ansi::NamedColor::DimGreen => self.dim_color(self.green),
            ansi::NamedColor::DimYellow => self.dim_color(self.yellow),
            ansi::NamedColor::DimBlue => self.dim_color(self.blue),
            ansi::NamedColor::DimMagenta => self.dim_color(self.magenta),
            ansi::NamedColor::DimCyan => self.dim_color(self.cyan),
            ansi::NamedColor::DimWhite => self.dim_color(self.white),
            ansi::NamedColor::BrightForeground => self.bright_color(self.foreground),
            ansi::NamedColor::DimForeground => self.dim_color(self.foreground),
        }
    }

    /// Brighten a palette color for bold variants.
    fn bright_color(&self, color: Color) -> Color {
        color.scale_brightness(1.2)
    }

    /// Dim a palette color for dim variants.
    fn dim_color(&self, color: Color) -> Color {
        color.scale_brightness(0.66)
    }

    /// Convert a vte RGB color into a Canopy color.
    fn rgb_color(rgb: ansi::Rgb) -> Color {
        Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }
    }

    /// Convert a Canopy color into a vte RGB payload.
    fn to_vte_rgb(color: Color) -> ansi::Rgb {
        let Color::Rgb { r, g, b } = color.to_rgb() else {
            unreachable!("Color::to_rgb always returns Color::Rgb");
        };
        ansi::Rgb { r, g, b }
    }
}

/// Terminal widget configuration.
pub struct TerminalConfig {
    /// Optional command argv to run instead of the default shell.
    pub command: Option<Vec<String>>,
    /// Working directory for the terminal process.
    pub cwd: Option<PathBuf>,
    /// Environment variables to inject into the terminal process.
    pub env: Vec<(String, String)>,
    /// Number of scrollback lines to keep.
    pub scrollback_lines: usize,
    /// Enable mouse reporting to the terminal when requested by the app.
    pub mouse_reporting: bool,
    /// Enable bracketed paste when requested by the app.
    pub bracketed_paste: bool,
    /// Color palette for the terminal.
    pub colors: TerminalColors,
    /// Optional callback invoked when the clipboard is updated.
    pub clipboard_store: Option<Arc<dyn Fn(String) + Send + Sync>>,
    /// Optional callback used to fetch clipboard contents.
    pub clipboard_load: Option<Arc<dyn Fn() -> String + Send + Sync>>,
    /// Optional callback invoked when the child process exits.
    pub on_exit: Option<Arc<dyn Fn(ExitStatus) + Send + Sync>>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            command: None,
            cwd: None,
            env: Vec::new(),
            scrollback_lines: DEFAULT_SCROLLBACK,
            mouse_reporting: true,
            bracketed_paste: true,
            colors: TerminalColors::default(),
            clipboard_store: None,
            clipboard_load: None,
            on_exit: None,
        }
    }
}

impl TerminalConfig {
    /// Construct a default terminal configuration.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Terminal widget backed by alacritty_terminal and portable-pty.
pub struct Terminal {
    /// User-provided configuration.
    config: TerminalConfig,
    /// Alacritty terminal state machine.
    term: Term<EventProxy>,
    /// VTE parser for incoming PTY bytes.
    parser: ansi::Processor,
    /// Buffered alacritty events for processing on poll.
    events: Arc<Mutex<Vec<alacritty_event::Event>>>,
    /// Shared buffer of PTY output bytes.
    read_buf: Arc<Mutex<Vec<u8>>>,
    /// Flag set when the reader thread hits EOF.
    reader_done: Arc<AtomicBool>,
    /// Join handle for the reader thread.
    reader_handle: Option<thread::JoinHandle<()>>,
    /// PTY master handle used for resize.
    master: Option<Box<dyn MasterPty + Send>>,
    /// Writer for PTY input.
    writer: Option<Box<dyn Write + Send>>,
    /// Child process handle.
    child: Option<Box<dyn Child + Send + Sync>>,
    /// Most recent terminal size.
    last_size: Option<TerminalSize>,
    /// Window size used for OSC size queries.
    window_size: alacritty_event::WindowSize,
    /// Cached child exit status.
    exit_status: Option<ExitStatus>,
    /// Whether the child process has exited.
    exited: bool,
    /// Cached cursor for rendering.
    cursor: Option<cursor::Cursor>,
    /// Local clipboard store for alacritty callbacks.
    clipboard: TerminalClipboard,
    /// Whether a selection drag is active.
    selection_active: bool,
    /// Multi-click tracking state.
    last_click: Option<ClickState>,
    /// App focus state from Canopy focus events.
    app_focused: bool,
    /// Current focus state reported to alacritty.
    focused: bool,
    /// Last reported terminal title.
    title: Option<String>,
}

#[derive_commands]
impl Terminal {
    /// Construct a new terminal widget with the provided configuration.
    pub fn new(config: TerminalConfig) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let event_proxy = EventProxy {
            events: events.clone(),
        };
        let size = TerminalSize::new(DEFAULT_COLUMNS, DEFAULT_LINES, config.scrollback_lines);
        let term_config = Config {
            scrolling_history: config.scrollback_lines,
            ..Config::default()
        };
        let term = Term::new(term_config, &size, event_proxy);
        let window_size = size.window_size();
        Self {
            config,
            term,
            parser: ansi::Processor::new(),
            events,
            read_buf: Arc::new(Mutex::new(Vec::new())),
            reader_done: Arc::new(AtomicBool::new(false)),
            reader_handle: None,
            master: None,
            writer: None,
            child: None,
            last_size: Some(size),
            window_size,
            exit_status: None,
            exited: false,
            cursor: None,
            clipboard: TerminalClipboard::default(),
            selection_active: false,
            last_click: None,
            app_focused: true,
            focused: false,
            title: None,
        }
    }

    /// Return the exit status of the child process, if it has exited.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.exit_status.clone()
    }

    /// Return true if the child process is still running.
    pub fn is_running(&self) -> bool {
        self.child.is_some() && !self.exited
    }

    /// Return the most recent terminal title, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Build the child process command line.
    fn build_command(&self) -> CommandBuilder {
        let mut cmd = if let Some(argv) = &self.config.command {
            if argv.is_empty() {
                CommandBuilder::new_default_prog()
            } else {
                let args: Vec<OsString> = argv.iter().map(OsString::from).collect();
                CommandBuilder::from_argv(args)
            }
        } else {
            CommandBuilder::new_default_prog()
        };

        if let Some(cwd) = &self.config.cwd {
            cmd.cwd(cwd);
        }
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        cmd
    }

    /// Ensure the terminal grid and PTY are resized to match the view.
    fn ensure_size(&mut self, expanse: geom::Expanse) {
        let size = TerminalSize::from_expanse(expanse, self.config.scrollback_lines);
        if self.last_size == Some(size) {
            return;
        }

        self.term.resize(size);
        if let Some(master) = self.master.as_ref()
            && master.resize(size.pty_size()).is_err()
        {
            // Ignore PTY resize failures; terminal state remains consistent.
        }
        self.window_size = size.window_size();
        self.last_size = Some(size);
    }

    /// Drain any pending PTY output captured by the reader thread.
    fn drain_read_buffer(&self) -> Vec<u8> {
        if let Ok(mut buf) = self.read_buf.lock() {
            buf.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Drain any queued terminal events from alacritty.
    fn drain_events(&self) -> Vec<alacritty_event::Event> {
        if let Ok(mut events) = self.events.lock() {
            events.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Process an alacritty terminal event.
    fn handle_term_event(&mut self, event: alacritty_event::Event) -> bool {
        match event {
            alacritty_event::Event::ClipboardStore(ty, text) => {
                self.clipboard.store(ty, text.clone());
                if let Some(callback) = &self.config.clipboard_store {
                    callback(text);
                }
                false
            }
            alacritty_event::Event::ClipboardLoad(ty, format) => {
                let content = if let Some(callback) = &self.config.clipboard_load {
                    callback()
                } else {
                    self.clipboard.load(ty).to_string()
                };
                let sequence = format(&content);
                self.write_to_pty(sequence.as_bytes());
                false
            }
            alacritty_event::Event::ColorRequest(index, format) => {
                let color = self
                    .config
                    .colors
                    .resolve_request_color(index, self.term.colors());
                let rgb = TerminalColors::to_vte_rgb(color);
                let sequence = format(rgb);
                self.write_to_pty(sequence.as_bytes());
                false
            }
            alacritty_event::Event::TextAreaSizeRequest(format) => {
                let sequence = format(self.window_size);
                self.write_to_pty(sequence.as_bytes());
                false
            }
            alacritty_event::Event::PtyWrite(text) => {
                self.write_to_pty(text.as_bytes());
                false
            }
            alacritty_event::Event::Title(title) => {
                self.title = Some(title);
                false
            }
            alacritty_event::Event::ResetTitle => {
                self.title = None;
                false
            }
            alacritty_event::Event::CursorBlinkingChange => true,
            alacritty_event::Event::MouseCursorDirty => false,
            alacritty_event::Event::Wakeup => true,
            alacritty_event::Event::Bell => false,
            alacritty_event::Event::Exit => false,
            alacritty_event::Event::ChildExit(_code) => true,
        }
    }

    /// Write bytes to the PTY master.
    fn write_to_pty(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        if writer.write_all(bytes).is_err() {
            return;
        }
        if writer.flush().is_err() {}
    }

    /// Record the child exit status and invoke the exit callback.
    fn note_exit(&mut self, status: ExitStatus) {
        if self.exited {
            return;
        }
        self.exited = true;
        self.exit_status = Some(status.clone());
        if let Some(callback) = &self.config.on_exit {
            callback(status);
        }
    }

    /// Copy the current selection to the clipboard and notify callbacks.
    fn copy_selection(&mut self) {
        let Some(text) = self.term.selection_to_string() else {
            return;
        };
        self.clipboard
            .store(term::ClipboardType::Clipboard, text.clone());
        if let Some(callback) = &self.config.clipboard_store {
            callback(text);
        }
    }

    /// Clear the terminal selection state.
    fn clear_selection(&mut self) {
        self.term.selection = None;
        self.selection_active = false;
    }

    /// Return whether the terminal should be considered focused.
    fn focus_state(&self, ctx: &dyn ReadContext) -> bool {
        ctx.is_focused() && self.app_focused
    }

    /// Sync focus state with the terminal and emit focus events.
    fn update_focus(&mut self, ctx: &dyn ReadContext) {
        let focused = self.focus_state(ctx);
        if focused == self.focused {
            self.term.is_focused = focused;
            return;
        }

        self.focused = focused;
        self.term.is_focused = focused;

        if self.term.mode().contains(TermMode::FOCUS_IN_OUT) {
            let sequence = if focused { "\x1b[I" } else { "\x1b[O" };
            self.write_to_pty(sequence.as_bytes());
        }
    }

    /// Translate a mouse location into a terminal grid point.
    fn selection_point(&self, location: geom::Point) -> Option<index::Point> {
        let size = self.last_size?;
        let cols = size.columns.max(1) as u32;
        let rows = size.screen_lines.max(1) as u32;
        let col = location.x.min(cols.saturating_sub(1)) as usize;
        let row = location.y.min(rows.saturating_sub(1)) as usize;
        let viewport = index::Point::new(row, index::Column(col));
        Some(term::viewport_to_point(
            self.term.grid().display_offset(),
            viewport,
        ))
    }

    /// Determine the selection mode based on click timing.
    fn selection_type_for_click(&mut self, location: geom::Point) -> SelectionType {
        let now = Instant::now();
        let threshold = Duration::from_millis(DOUBLE_CLICK_MS);
        let mut count = 1;

        if let Some(state) = self.last_click.as_mut() {
            if state.location == location && now.duration_since(state.last_click) <= threshold {
                state.count = state.count.saturating_add(1).min(3);
                state.last_click = now;
                count = state.count;
            } else {
                state.location = location;
                state.count = 1;
                state.last_click = now;
            }
        } else {
            self.last_click = Some(ClickState {
                location,
                last_click: now,
                count: 1,
            });
        }

        match count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        }
    }

    /// Begin a selection at the provided location.
    fn handle_selection_start(&mut self, location: geom::Point) -> bool {
        let Some(point) = self.selection_point(location) else {
            return false;
        };

        let selection_type = self.selection_type_for_click(location);
        self.term.selection = Some(Selection::new(selection_type, point, index::Side::Left));
        self.selection_active = true;
        true
    }

    /// Update the active selection while dragging.
    fn handle_selection_update(&mut self, location: geom::Point) -> bool {
        if !self.selection_active {
            return false;
        }

        let Some(point) = self.selection_point(location) else {
            return false;
        };

        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, index::Side::Right);
            return true;
        }

        false
    }

    /// Finalize the current selection.
    fn handle_selection_end(&mut self) -> bool {
        if !self.selection_active {
            return false;
        }

        if let Some(selection) = self.term.selection.as_mut() {
            selection.include_all();
        }
        self.selection_active = false;
        true
    }

    /// Send pasted content to the PTY, respecting bracketed paste.
    fn handle_paste(&mut self, content: &str) {
        let mut bytes = Vec::new();
        let bracketed =
            self.config.bracketed_paste && self.term.mode().contains(TermMode::BRACKETED_PASTE);
        if bracketed {
            bytes.extend_from_slice(b"\x1b[200~");
        }
        bytes.extend_from_slice(content.as_bytes());
        if bracketed {
            bytes.extend_from_slice(b"\x1b[201~");
        }
        self.write_to_pty(&bytes);
    }

    /// Encode a key event into terminal escape sequences.
    fn encode_key(&self, key: &key::Key) -> Option<Vec<u8>> {
        use key::KeyCode;

        let app_cursor = self.term.mode().contains(TermMode::APP_CURSOR);
        let mut out = Vec::new();

        if key.mods.alt {
            out.push(0x1b);
        }

        match key.key {
            KeyCode::Enter => out.push(b'\r'),
            KeyCode::Tab => out.push(b'\t'),
            KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
            KeyCode::Backspace => out.push(0x7f),
            KeyCode::Esc => out.push(0x1b),
            KeyCode::Left => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOD");
                } else {
                    out.extend_from_slice(b"\x1b[D");
                }
            }
            KeyCode::Right => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOC");
                } else {
                    out.extend_from_slice(b"\x1b[C");
                }
            }
            KeyCode::Up => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOA");
                } else {
                    out.extend_from_slice(b"\x1b[A");
                }
            }
            KeyCode::Down => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOB");
                } else {
                    out.extend_from_slice(b"\x1b[B");
                }
            }
            KeyCode::Home => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOH");
                } else {
                    out.extend_from_slice(b"\x1b[H");
                }
            }
            KeyCode::End => {
                if app_cursor {
                    out.extend_from_slice(b"\x1bOF");
                } else {
                    out.extend_from_slice(b"\x1b[F");
                }
            }
            KeyCode::Insert => out.extend_from_slice(b"\x1b[2~"),
            KeyCode::Delete => out.extend_from_slice(b"\x1b[3~"),
            KeyCode::PageUp => out.extend_from_slice(b"\x1b[5~"),
            KeyCode::PageDown => out.extend_from_slice(b"\x1b[6~"),
            KeyCode::F(n) => match n {
                1 => out.extend_from_slice(b"\x1bOP"),
                2 => out.extend_from_slice(b"\x1bOQ"),
                3 => out.extend_from_slice(b"\x1bOR"),
                4 => out.extend_from_slice(b"\x1bOS"),
                5 => out.extend_from_slice(b"\x1b[15~"),
                6 => out.extend_from_slice(b"\x1b[17~"),
                7 => out.extend_from_slice(b"\x1b[18~"),
                8 => out.extend_from_slice(b"\x1b[19~"),
                9 => out.extend_from_slice(b"\x1b[20~"),
                10 => out.extend_from_slice(b"\x1b[21~"),
                11 => out.extend_from_slice(b"\x1b[23~"),
                12 => out.extend_from_slice(b"\x1b[24~"),
                _ => return None,
            },
            KeyCode::Char(c) => {
                if key.mods.ctrl {
                    if let Some(ctrl) = Self::ctrl_char(c) {
                        out.push(ctrl);
                    } else {
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                } else {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            _ => return None,
        }

        Some(out)
    }

    /// Encode a control character if supported.
    fn ctrl_char(c: char) -> Option<u8> {
        let upper = c.to_ascii_uppercase();
        match upper {
            '@' | ' ' => Some(0),
            'A'..='Z' => Some((upper as u8) - b'A' + 1),
            '[' => Some(27),
            '\\' => Some(28),
            ']' => Some(29),
            '^' => Some(30),
            '_' => Some(31),
            _ => None,
        }
    }

    /// Encode mouse events into terminal escape sequences.
    fn encode_mouse(&self, event: &mouse::MouseEvent, mode: TermMode) -> Option<Vec<u8>> {
        let cols = self.term.columns().max(1) as u32;
        let rows = self.term.screen_lines().max(1) as u32;
        let x = event.location.x.min(cols.saturating_sub(1)) + 1;
        let y = event.location.y.min(rows.saturating_sub(1)) + 1;

        let mut cb = match event.action {
            mouse::Action::ScrollUp => 64,
            mouse::Action::ScrollDown => 65,
            mouse::Action::ScrollLeft => 66,
            mouse::Action::ScrollRight => 67,
            _ => match event.button {
                mouse::Button::Left => 0,
                mouse::Button::Middle => 1,
                mouse::Button::Right => 2,
                mouse::Button::None => 3,
            },
        };

        if event.action == mouse::Action::Up {
            cb = 3;
        }

        if matches!(event.action, mouse::Action::Moved | mouse::Action::Drag) {
            cb |= 32;
        }

        if event.modifiers.shift {
            cb |= 4;
        }
        if event.modifiers.alt {
            cb |= 8;
        }
        if event.modifiers.ctrl {
            cb |= 16;
        }

        if mode.contains(TermMode::SGR_MOUSE) {
            let suffix = if event.action == mouse::Action::Up {
                'm'
            } else {
                'M'
            };
            let sequence = format!("\x1b[<{cb};{x};{y}{suffix}");
            return Some(sequence.into_bytes());
        }

        let cb = (cb + 32).min(255);
        let x = (x + 32).min(255) as u8;
        let y = (y + 32).min(255) as u8;
        Some(vec![0x1b, b'[', b'M', cb as u8, x, y])
    }
}

impl Widget for Terminal {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();
        let content_size = view.content_size();
        if content_size.w == 0 || content_size.h == 0 {
            self.cursor = None;
            return Ok(());
        }

        self.ensure_size(content_size);
        self.update_focus(ctx);

        let renderable = self.term.renderable_content();
        let display_offset = renderable.display_offset;
        let selection = renderable.selection;
        let cursor_shape = renderable.cursor.shape;
        let blink = self.term.cursor_style().blinking;

        self.cursor = None;
        if self.focus_state(ctx) && cursor_shape != ansi::CursorShape::Hidden {
            let point = term::point_to_viewport(display_offset, renderable.cursor.point);
            if let Some(point) = point {
                let col = point.column.0 as u32;
                let row = point.line as u32;
                if col < content_size.w && row < content_size.h {
                    let shape = match cursor_shape {
                        ansi::CursorShape::Block | ansi::CursorShape::HollowBlock => {
                            cursor::CursorShape::Block
                        }
                        ansi::CursorShape::Underline => cursor::CursorShape::Underscore,
                        ansi::CursorShape::Beam => cursor::CursorShape::Line,
                        ansi::CursorShape::Hidden => cursor::CursorShape::Block,
                    };
                    self.cursor = Some(cursor::Cursor {
                        location: geom::Point { x: col, y: row },
                        shape,
                        blink,
                    });
                }
            }
        }

        for indexed in renderable.display_iter {
            let line = indexed.point.line.0 + display_offset as i32;
            if line < 0 {
                continue;
            }
            let row = line as u32;
            if row >= content_size.h {
                continue;
            }
            let col = indexed.point.column.0 as u32;
            if col >= content_size.w {
                continue;
            }

            let mut ch = indexed.cell.c;
            let flags = indexed.cell.flags;
            let mut fg = self
                .config
                .colors
                .resolve_color(indexed.cell.fg, renderable.colors);
            let mut bg = self
                .config
                .colors
                .resolve_color(indexed.cell.bg, renderable.colors);

            if flags.contains(term::cell::Flags::INVERSE) {
                mem::swap(&mut fg, &mut bg);
            }

            // Attribute mapping: bold/italic/underline/strike/dim -> AttrSet,
            // inverse swaps fg/bg, and hidden renders as a space.
            let mut attrs = AttrSet::default();
            if flags.contains(term::cell::Flags::BOLD) {
                attrs.bold = true;
            }
            if flags.contains(term::cell::Flags::ITALIC) {
                attrs.italic = true;
            }
            if flags.contains(term::cell::Flags::DIM) {
                attrs.dim = true;
            }
            if flags.contains(term::cell::Flags::STRIKEOUT) {
                attrs.crossedout = true;
            }
            if flags.intersects(term::cell::Flags::ALL_UNDERLINES) {
                attrs.underline = true;
            }

            if flags.contains(term::cell::Flags::HIDDEN) {
                ch = ' ';
            }

            if let Some(selection) = selection
                && selection.contains_cell(&indexed, indexed.point, cursor_shape)
            {
                mem::swap(&mut fg, &mut bg);
            }

            if flags.contains(term::cell::Flags::WIDE_CHAR_SPACER)
                || flags.contains(term::cell::Flags::LEADING_WIDE_CHAR_SPACER)
            {
                ch = ' ';
            }

            let style = Style { fg, bg, attrs };
            let local = geom::Point {
                x: view.content_origin().x.saturating_add(col),
                y: view.content_origin().y.saturating_add(row),
            };
            rndr.put_cell(style, local, ch)?;
        }

        if self.exited {
            let status = self
                .exit_status
                .as_ref()
                .map(|s| s.exit_code())
                .unwrap_or(1);
            let message = format!("Process exited (status {status})");
            let width = message.chars().count() as u32;
            if width > 0 && content_size.w > 0 && content_size.h > 0 {
                let x = (content_size.w.saturating_sub(width)) / 2;
                let y = content_size.h / 2;
                let line = geom::Line::new(
                    view.content_origin().x.saturating_add(x),
                    view.content_origin().y.saturating_add(y),
                    width.min(content_size.w),
                );
                rndr.text("text", line, &message)?;
            }
        }

        Ok(())
    }

    fn on_event(&mut self, event: &event::Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        match event {
            event::Event::Key(key) => {
                if key.mods.shift {
                    match key.key {
                        key::KeyCode::PageUp => {
                            self.term.scroll_display(Scroll::PageUp);
                            return Ok(EventOutcome::Handle);
                        }
                        key::KeyCode::PageDown => {
                            self.term.scroll_display(Scroll::PageDown);
                            return Ok(EventOutcome::Handle);
                        }
                        _ => {}
                    }
                }

                if key.mods.ctrl
                    && key.mods.shift
                    && matches!(key.key, key::KeyCode::Char('c' | 'C'))
                {
                    self.copy_selection();
                    return Ok(EventOutcome::Handle);
                }

                if let Some(bytes) = self.encode_key(key)
                    && !bytes.is_empty()
                {
                    self.clear_selection();
                    self.write_to_pty(&bytes);
                    return Ok(EventOutcome::Handle);
                }

                Ok(EventOutcome::Ignore)
            }
            event::Event::Paste(content) => {
                self.clear_selection();
                self.handle_paste(content);
                Ok(EventOutcome::Handle)
            }
            event::Event::Mouse(m) => {
                ctx.set_focus(ctx.node_id());

                let mode = *self.term.mode();
                if self.config.mouse_reporting && mode.contains(TermMode::MOUSE_MODE) {
                    let allow_motion = mode.contains(TermMode::MOUSE_MOTION);
                    let allow_drag = mode.contains(TermMode::MOUSE_DRAG);
                    let send = match m.action {
                        mouse::Action::Moved => allow_motion,
                        mouse::Action::Drag => allow_motion || allow_drag,
                        _ => true,
                    };
                    if send && let Some(seq) = self.encode_mouse(m, mode) {
                        self.write_to_pty(&seq);
                    }
                    return Ok(EventOutcome::Handle);
                }

                let outcome = match m.action {
                    mouse::Action::ScrollUp => {
                        self.term.scroll_display(Scroll::Delta(SCROLL_LINES));
                        EventOutcome::Handle
                    }
                    mouse::Action::ScrollDown => {
                        self.term.scroll_display(Scroll::Delta(-SCROLL_LINES));
                        EventOutcome::Handle
                    }
                    mouse::Action::Down if m.button == mouse::Button::Left => {
                        if self.handle_selection_start(m.location) {
                            EventOutcome::Handle
                        } else {
                            EventOutcome::Ignore
                        }
                    }
                    mouse::Action::Drag if m.button == mouse::Button::Left => {
                        if self.handle_selection_update(m.location) {
                            EventOutcome::Handle
                        } else {
                            EventOutcome::Ignore
                        }
                    }
                    mouse::Action::Up if m.button == mouse::Button::Left => {
                        if self.handle_selection_end() {
                            EventOutcome::Handle
                        } else {
                            EventOutcome::Ignore
                        }
                    }
                    _ => EventOutcome::Ignore,
                };
                Ok(outcome)
            }
            event::Event::FocusGained => {
                self.app_focused = true;
                self.update_focus(ctx);
                Ok(EventOutcome::Handle)
            }
            event::Event::FocusLost => {
                self.app_focused = false;
                self.update_focus(ctx);
                Ok(EventOutcome::Handle)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.wrap()
    }

    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        view
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        self.cursor.clone()
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
        if self.exited {
            return None;
        }

        let bytes = self.drain_read_buffer();
        if !bytes.is_empty() {
            self.parser.advance(&mut self.term, &bytes);
        }

        for event in self.drain_events() {
            let _ = self.handle_term_event(event);
        }

        if self.reader_done.load(Ordering::SeqCst) {
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => self.note_exit(status),
                    Ok(None) => {}
                    Err(_) => {
                        self.note_exit(ExitStatus::with_exit_code(1));
                    }
                }
            } else if !self.exited {
                self.exited = true;
            }
        }

        Some(Duration::from_millis(POLL_INTERVAL_MS))
    }

    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        let size = self.last_size.unwrap_or_else(|| {
            TerminalSize::new(DEFAULT_COLUMNS, DEFAULT_LINES, self.config.scrollback_lines)
        });
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(size.pty_size())
            .map_err(|e| Error::Internal(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| Error::Internal(e.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| Error::Internal(e.to_string()))?;

        let cmd = self.build_command();
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| Error::Internal(e.to_string()))?;

        let read_buf = self.read_buf.clone();
        let reader_done = self.reader_done.clone();
        let handle = thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        reader_done.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(n) => {
                        if let Ok(mut out) = read_buf.lock() {
                            out.extend_from_slice(&buf[..n]);
                        }
                    }
                    Err(_) => {
                        reader_done.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });

        self.reader_handle = Some(handle);
        self.master = Some(pair.master);
        self.writer = Some(writer);
        self.child = Some(child);

        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("terminal")
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take()
            && child.kill().is_err()
        {}
        self.writer.take();
        if let Some(handle) = self.reader_handle.take()
            && handle.join().is_err()
        {}
    }
}
