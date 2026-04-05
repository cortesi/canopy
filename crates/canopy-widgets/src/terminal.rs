use std::{
    mem,
    path::PathBuf,
    result::Result as StdResult,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use canopy::{
    Context, EventOutcome, ReadContext, Widget, cursor, derive_commands,
    error::{Error, Result},
    event::{self, key, mouse},
    geom,
    layout::{CanvasContext, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    style::{AttrSet, Color, ResolvedStyle},
};
use itty::{
    Session, ViewportSelection,
    clipboard::ClipboardHandler,
    config::{EguiTTYConfig, EguiTTYConfigBuilder, Hex, PaletteConfig, PaletteKind, PaletteMeta},
    driver::{self, DriverHandle, DriverHost},
    inspect::{StyledRunPublic, TerminalState},
    key::{Key as IttyKey, KeyCode as IttyKeyCode, Modifiers as IttyModifiers},
    title::TitleHook,
};
use portable_pty::ExitStatus;
use tokio::runtime::{Builder, Runtime};
use unicode_width::UnicodeWidthChar;

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

/// Track click timing for selection behavior.
struct ClickState {
    /// Last click location.
    location: geom::Point,
    /// Last click timestamp.
    last_click: Instant,
    /// Number of clicks in the current multi-click sequence.
    count: u8,
}

/// Shared clipboard shim that bridges Canopy callbacks into `itty`.
struct SharedClipboard {
    /// Stored clipboard contents when no external callback is installed.
    text: Mutex<String>,
    /// Optional callback invoked when text is stored.
    store: Option<Arc<dyn Fn(String) + Send + Sync>>,
    /// Optional callback invoked when text is loaded.
    load: Option<Arc<dyn Fn() -> String + Send + Sync>>,
}

impl SharedClipboard {
    /// Construct a clipboard bridge from the widget config.
    fn new(config: &TerminalConfig) -> Arc<Self> {
        Arc::new(Self {
            text: Mutex::new(String::new()),
            store: config.clipboard_store.clone(),
            load: config.clipboard_load.clone(),
        })
    }
}

impl ClipboardHandler for SharedClipboard {
    fn set_text(&self, text: &str) -> StdResult<(), String> {
        if let Some(store) = &self.store {
            store(text.to_string());
        }
        let mut guard = self.text.lock().map_err(|error| error.to_string())?;
        *guard = text.to_string();
        Ok(())
    }

    fn get_text(&self) -> StdResult<String, String> {
        if let Some(load) = &self.load {
            return Ok(load());
        }
        let guard = self.text.lock().map_err(|error| error.to_string())?;
        Ok(guard.clone())
    }
}

/// Shared title hook used to surface title updates from `itty`.
struct SharedTitle {
    /// Most recent title emitted by the backend.
    title: Arc<Mutex<Option<String>>>,
}

/// Send wrapper around the thread-affine driver host.
struct DriverPortal {
    /// Host polled from the UI thread.
    host: DriverHost,
}

// SAFETY: `DriverHost` already records and asserts the thread it is polled on.
// Canopy requires widgets to be `Send`, but this wrapper does not relax the
// actual thread-affinity checks enforced by the host itself.
unsafe impl Send for DriverPortal {}

impl TitleHook for SharedTitle {
    fn set_title(&self, title: &str) {
        if let Ok(mut guard) = self.title.lock() {
            *guard = Some(title.to_string());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Terminal grid sizing metadata.
struct TerminalSize {
    /// Visible terminal columns.
    columns: usize,
    /// Visible terminal rows.
    rows: usize,
}

impl TerminalSize {
    /// Convert a Canopy expanse into a terminal grid size.
    fn from_expanse(expanse: geom::Size) -> Self {
        Self {
            columns: expanse.w.max(1) as usize,
            rows: expanse.h.max(1) as usize,
        }
    }
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
    /// Convert the Canopy palette into an inline `itty` palette.
    fn palette_config(self) -> PaletteConfig {
        PaletteConfig {
            normal: [
                canopy_hex(self.black),
                canopy_hex(self.red),
                canopy_hex(self.green),
                canopy_hex(self.yellow),
                canopy_hex(self.blue),
                canopy_hex(self.magenta),
                canopy_hex(self.cyan),
                canopy_hex(self.white),
            ],
            bright: [
                canopy_hex(self.bright_black),
                canopy_hex(self.bright_red),
                canopy_hex(self.bright_green),
                canopy_hex(self.bright_yellow),
                canopy_hex(self.bright_blue),
                canopy_hex(self.bright_magenta),
                canopy_hex(self.bright_cyan),
                canopy_hex(self.bright_white),
            ],
            dim: None,
            foreground: canopy_hex(self.foreground),
            background: canopy_hex(self.background),
            cursor: canopy_hex(self.cursor),
            bright_foreground: None,
            dim_foreground: None,
            selection_bg: None,
            selection_fg: None,
            search_match_bg: None,
            search_current_bg: None,
            meta: PaletteMeta {
                name: "canopy".to_string(),
                kind: PaletteKind::Dark,
            },
        }
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
    /// Enable kitty keyboard protocol support.
    pub kitty_keyboard: bool,
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
            kitty_keyboard: true,
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

/// Terminal widget backed by `itty`.
pub struct Terminal {
    /// User-provided configuration.
    config: TerminalConfig,
    /// Backend terminal session.
    session: Option<Session>,
    /// Driver host polled from Canopy's UI loop.
    driver_host: Option<DriverPortal>,
    /// Cloneable driver handle exposed to integrations.
    driver_handle: Option<Arc<DriverHandle>>,
    /// Runtime used to enqueue async driver operations without blocking UI events.
    driver_runtime: Option<Runtime>,
    /// Most recent terminal size.
    last_size: TerminalSize,
    /// Cached cursor for rendering.
    cursor: Option<cursor::Cursor>,
    /// Whether a selection drag is active.
    selection_active: bool,
    /// Selection anchor in viewport coordinates.
    selection_anchor: Option<geom::Point>,
    /// Multi-click tracking state.
    last_click: Option<ClickState>,
    /// App focus state from Canopy focus events.
    app_focused: bool,
    /// Last reported terminal title.
    title: Arc<Mutex<Option<String>>>,
    /// Whether the child exit callback has been invoked.
    exit_notified: bool,
    /// Cached child exit status.
    exit_status: Option<ExitStatus>,
}

#[derive_commands]
impl Terminal {
    /// Construct a new terminal widget with the provided configuration.
    pub fn new(config: TerminalConfig) -> Self {
        Self {
            config,
            session: None,
            driver_host: None,
            driver_handle: None,
            driver_runtime: None,
            last_size: TerminalSize {
                columns: DEFAULT_COLUMNS,
                rows: DEFAULT_LINES,
            },
            cursor: None,
            selection_active: false,
            selection_anchor: None,
            last_click: None,
            app_focused: true,
            title: Arc::new(Mutex::new(None)),
            exit_notified: false,
            exit_status: None,
        }
    }

    /// Return the exit status of the child process, if it has exited.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.exit_status.clone()
    }

    /// Return true if the child process is still running.
    pub fn is_running(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| !session.child_exited())
    }

    /// Return the most recent terminal title, if any.
    pub fn title(&self) -> Option<String> {
        self.title.lock().ok().and_then(|guard| guard.clone())
    }

    /// Return the attached `itty` driver handle for scripting integrations.
    pub fn driver_handle(&self) -> Option<Arc<DriverHandle>> {
        self.driver_handle.as_ref().map(Arc::clone)
    }

    /// Lazily create the backend session and driver bridge.
    fn mount_session(&mut self) -> Result<()> {
        if self.session.is_some() {
            return Ok(());
        }

        let cfg = terminal_config(&self.config, self.last_size);
        let mut session =
            Session::from_config(&cfg).map_err(|error| Error::Internal(error.to_string()))?;
        session.set_clipboard_handler(SharedClipboard::new(&self.config));
        session.set_title_hook(Arc::new(SharedTitle {
            title: Arc::clone(&self.title),
        }));

        let (host, handle) = driver::attach(&mut session);
        let runtime = Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| Error::Internal(error.to_string()))?;

        self.exit_notified = false;
        self.exit_status = None;
        self.session = Some(session);
        self.driver_host = Some(DriverPortal { host });
        self.driver_handle = Some(Arc::new(handle));
        self.driver_runtime = Some(runtime);
        Ok(())
    }

    /// Borrow the live backend session.
    fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Borrow the live backend session mutably.
    fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// Drive any pending backend work once from Canopy's poll loop.
    fn poll_driver(&mut self) {
        if let (Some(portal), Some(session)) = (self.driver_host.as_mut(), self.session.as_mut()) {
            let _ = portal.host.poll_nonblocking(session);
        }
    }

    /// Ensure the terminal grid matches the current view.
    fn ensure_size(&mut self, expanse: geom::Size) {
        let size = TerminalSize::from_expanse(expanse);
        if self.last_size == size {
            return;
        }

        self.last_size = size;
        if let Some(session) = self.session.as_mut()
            && let Err(_error) = session.resize_grid_and_pty(size.columns, size.rows, 1.0)
        {}
    }

    /// Return the current backend state snapshot.
    fn state(&self) -> Option<TerminalState> {
        self.session().map(Session::state)
    }

    /// Clear the current viewport selection overlay.
    fn clear_selection(&mut self) {
        if let Some(session) = self.session_mut() {
            session.set_viewport_selection(None);
        }
        self.selection_active = false;
        self.selection_anchor = None;
    }

    /// Update the viewport selection overlay between two points.
    fn set_selection(&mut self, start: geom::Point, end: geom::Point) -> bool {
        let Some(session) = self.session_mut() else {
            return false;
        };

        session.set_viewport_selection(Some(itty::ViewportSelection {
            start_row: start.y as usize,
            start_col: start.x as usize,
            end_row: end.y as usize,
            end_col: end.x as usize,
            block: false,
        }));
        true
    }

    /// Translate a mouse location into a clamped terminal grid point.
    fn selection_point(&self, location: geom::Point) -> Option<geom::Point> {
        let state = self.state()?;
        let x = location.x.min(state.cols.saturating_sub(1) as u32);
        let y = location.y.min(state.lines.saturating_sub(1) as u32);
        Some(geom::Point { x, y })
    }

    /// Determine the selection mode based on click timing.
    fn selection_type_for_click(&mut self, location: geom::Point) -> u8 {
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

        count
    }

    /// Select a single semantic word around the provided viewport point.
    fn select_word(&mut self, point: geom::Point) -> bool {
        let Some(line) = self
            .session()
            .and_then(|session| session.visible_text().get(point.y as usize).cloned())
        else {
            return false;
        };

        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return self.set_selection(point, point);
        }

        let mut idx = (point.x as usize).min(chars.len().saturating_sub(1));
        if chars[idx].is_whitespace() {
            return self.set_selection(point, point);
        }

        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        let start = idx;
        let mut end = (point.x as usize).min(chars.len().saturating_sub(1));
        while end + 1 < chars.len() && !chars[end + 1].is_whitespace() {
            end += 1;
        }

        self.set_selection(
            geom::Point {
                x: start as u32,
                y: point.y,
            },
            geom::Point {
                x: end as u32,
                y: point.y,
            },
        )
    }

    /// Select the full viewport line containing the provided point.
    fn select_line(&mut self, point: geom::Point) -> bool {
        let Some(state) = self.state() else {
            return false;
        };
        self.set_selection(
            geom::Point { x: 0, y: point.y },
            geom::Point {
                x: state.cols.saturating_sub(1) as u32,
                y: point.y,
            },
        )
    }

    /// Begin a selection at the provided location.
    fn handle_selection_start(&mut self, location: geom::Point) -> bool {
        let Some(point) = self.selection_point(location) else {
            return false;
        };

        match self.selection_type_for_click(point) {
            2 => {
                self.selection_active = false;
                self.selection_anchor = None;
                self.select_word(point)
            }
            3 => {
                self.selection_active = false;
                self.selection_anchor = None;
                self.select_line(point)
            }
            _ => {
                self.selection_active = true;
                self.selection_anchor = Some(point);
                self.set_selection(point, point)
            }
        }
    }

    /// Update the active selection while dragging.
    fn handle_selection_update(&mut self, location: geom::Point) -> bool {
        if !self.selection_active {
            return false;
        }

        let Some(anchor) = self.selection_anchor else {
            return false;
        };
        let Some(point) = self.selection_point(location) else {
            return false;
        };
        self.set_selection(anchor, point)
    }

    /// Finalize the current selection.
    fn handle_selection_end(&mut self) -> bool {
        if !self.selection_active {
            return false;
        }
        self.selection_active = false;
        true
    }

    /// Queue raw input bytes through the attached driver without blocking the UI thread.
    fn queue_input(&self, bytes: Vec<u8>) {
        let (Some(runtime), Some(handle)) = (&self.driver_runtime, &self.driver_handle) else {
            return;
        };
        let handle = Arc::clone(handle);
        mem::drop(runtime.spawn(async move {
            drop(handle.send_input(bytes).await);
        }));
    }

    /// Send a mouse input sequence to the terminal when mouse reporting is enabled.
    fn send_mouse_sequence(&self, event: &mouse::MouseEvent, state: &TerminalState) {
        if let Some(bytes) = encode_mouse(event, state) {
            self.queue_input(bytes);
        }
    }

    /// Copy the current selection to the configured clipboard callback.
    fn copy_selection(&self) {
        let Some(text) = self.session().and_then(Session::copy_selection) else {
            return;
        };
        if let Some(store) = &self.config.clipboard_store {
            store(text);
        }
    }

    /// Send pasted content to the PTY, optionally bypassing bracketed paste.
    fn handle_paste(&self, content: &str) {
        let Some(session) = self.session() else {
            return;
        };

        if self.config.bracketed_paste {
            drop(session.paste(content));
            return;
        }

        self.queue_input(content.as_bytes().to_vec());
    }

    /// Encode and send a keyboard event to the backend session.
    fn handle_key(&mut self, key: key::Key) -> bool {
        if key.mods.shift {
            match key.key {
                key::KeyCode::PageUp => {
                    if let Some(session) = self.session_mut() {
                        session.scroll_page_up();
                    }
                    return true;
                }
                key::KeyCode::PageDown => {
                    if let Some(session) = self.session_mut() {
                        session.scroll_page_down();
                    }
                    return true;
                }
                _ => {}
            }
        }

        if key.mods.ctrl && key.mods.shift && matches!(key.key, key::KeyCode::Char('c' | 'C')) {
            self.copy_selection();
            return true;
        }

        let Some(mapped) = map_key(key) else {
            return false;
        };

        self.clear_selection();
        if let Some(session) = self.session() {
            drop(session.send_key(mapped));
            return true;
        }
        false
    }

    /// Sync exit bookkeeping and invoke the configured callback exactly once.
    fn sync_exit_status(&mut self) {
        let Some(session) = self.session() else {
            return;
        };
        if !session.child_exited() || self.exit_notified {
            return;
        }

        let code = session.child_exit_code().unwrap_or(1).max(0) as u32;
        let status = ExitStatus::with_exit_code(code);
        self.exit_status = Some(status.clone());
        self.exit_notified = true;
        if let Some(callback) = &self.config.on_exit {
            callback(status);
        }
    }

    /// Sync terminal focus reporting with the backend.
    fn sync_focus(&self, focused: bool) {
        let Some(state) = self.state() else {
            return;
        };
        if !state.modes.focus_in_out {
            return;
        }

        let bytes = if focused {
            b"\x1b[I".to_vec()
        } else {
            b"\x1b[O".to_vec()
        };
        self.queue_input(bytes);
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
        let Some(session) = self.session() else {
            return Ok(());
        };

        let state = session.state();
        let runs = session.visible_runs();
        let selection = session.viewport_selection();
        let child_exited = session.child_exited();
        let child_exit_code = session.child_exit_code().unwrap_or(1);
        let default_bg = self.config.colors.background;
        self.cursor = cursor_from_state(&state);

        for (row_idx, line) in runs.iter().enumerate() {
            for run in line {
                render_run(
                    rndr,
                    view.content_origin(),
                    row_idx,
                    run,
                    selection,
                    default_bg,
                )?;
            }
        }

        if child_exited {
            let status = child_exit_code;
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
        if self.session.is_none() {
            return Ok(EventOutcome::Ignore);
        }

        match event {
            event::Event::Key(key) => {
                if self.handle_key(*key) {
                    Ok(EventOutcome::Handle)
                } else {
                    Ok(EventOutcome::Ignore)
                }
            }
            event::Event::Paste(content) => {
                self.clear_selection();
                self.handle_paste(content);
                Ok(EventOutcome::Handle)
            }
            event::Event::Mouse(mouse_event) => {
                ctx.set_focus(ctx.node_id());
                let Some(state) = self.state() else {
                    return Ok(EventOutcome::Ignore);
                };

                let mouse_reporting = self.config.mouse_reporting
                    && (state.modes.mouse_report_click
                        || state.modes.mouse_drag
                        || state.modes.mouse_motion);
                if mouse_reporting {
                    self.send_mouse_sequence(mouse_event, &state);
                    return Ok(EventOutcome::Handle);
                }

                let outcome = match mouse_event.action {
                    mouse::Action::ScrollUp => {
                        if let Some(session) = self.session_mut() {
                            session.scroll_delta(session.scroll_wheel_step());
                        }
                        EventOutcome::Handle
                    }
                    mouse::Action::ScrollDown => {
                        if let Some(session) = self.session_mut() {
                            session.scroll_delta(-session.scroll_wheel_step());
                        }
                        EventOutcome::Handle
                    }
                    mouse::Action::Down if mouse_event.button == mouse::Button::Left => {
                        if self.handle_selection_start(mouse_event.location) {
                            EventOutcome::Handle
                        } else {
                            EventOutcome::Ignore
                        }
                    }
                    mouse::Action::Drag if mouse_event.button == mouse::Button::Left => {
                        if self.handle_selection_update(mouse_event.location) {
                            EventOutcome::Handle
                        } else {
                            EventOutcome::Ignore
                        }
                    }
                    mouse::Action::Up if mouse_event.button == mouse::Button::Left => {
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
                self.sync_focus(true);
                Ok(EventOutcome::Handle)
            }
            event::Event::FocusLost => {
                self.app_focused = false;
                self.sync_focus(false);
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
        self.poll_driver();
        self.sync_exit_status();
        Some(Duration::from_millis(POLL_INTERVAL_MS))
    }

    fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
        self.mount_session()
    }

    fn name(&self) -> NodeName {
        NodeName::convert("terminal")
    }
}

/// Convert a Canopy color into an `itty` hex wrapper.
fn canopy_hex(color: Color) -> Hex {
    let Color::Rgb { r, g, b } = color.to_rgb() else {
        unreachable!("Color::to_rgb always returns Color::Rgb");
    };
    Hex::from_rgb(itty::Rgb { r, g, b })
}

/// Build an `itty` config from Canopy's terminal config.
fn terminal_config(config: &TerminalConfig, size: TerminalSize) -> EguiTTYConfig {
    let mut builder = EguiTTYConfigBuilder::new()
        .grid_fixed(size.columns, size.rows)
        .scrollback_lines(config.scrollback_lines)
        .kitty_keyboard(config.kitty_keyboard)
        .palette_inline(config.colors.palette_config());

    if let Some(argv) = &config.command
        && let Some((program, args)) = argv.split_first()
    {
        builder = builder
            .pty_shell(program.clone())
            .pty_args(args.iter().cloned());
    }
    if let Some(cwd) = &config.cwd {
        builder = builder.pty_working_dir(cwd.display().to_string());
    }
    for (key, value) in &config.env {
        builder = builder.pty_env_var(key.clone(), value.clone());
    }
    builder.build()
}

/// Convert a Canopy key into an `itty` key.
fn map_key(key: key::Key) -> Option<IttyKey> {
    let mut modifiers = IttyModifiers::empty();
    if key.mods.shift {
        modifiers |= IttyModifiers::SHIFT;
    }
    if key.mods.ctrl {
        modifiers |= IttyModifiers::CTRL;
    }
    if key.mods.alt {
        modifiers |= IttyModifiers::ALT;
    }

    let code = match key.key {
        key::KeyCode::Backspace => IttyKeyCode::Backspace,
        key::KeyCode::Enter => IttyKeyCode::Enter,
        key::KeyCode::Left => IttyKeyCode::ArrowLeft,
        key::KeyCode::Right => IttyKeyCode::ArrowRight,
        key::KeyCode::Up => IttyKeyCode::ArrowUp,
        key::KeyCode::Down => IttyKeyCode::ArrowDown,
        key::KeyCode::Home => IttyKeyCode::Home,
        key::KeyCode::End => IttyKeyCode::End,
        key::KeyCode::PageUp => IttyKeyCode::PageUp,
        key::KeyCode::PageDown => IttyKeyCode::PageDown,
        key::KeyCode::Tab | key::KeyCode::BackTab => IttyKeyCode::Tab,
        key::KeyCode::Delete => IttyKeyCode::Delete,
        key::KeyCode::Insert => IttyKeyCode::Insert,
        key::KeyCode::Esc => IttyKeyCode::Escape,
        key::KeyCode::F(1) => IttyKeyCode::F1,
        key::KeyCode::F(2) => IttyKeyCode::F2,
        key::KeyCode::F(3) => IttyKeyCode::F3,
        key::KeyCode::F(4) => IttyKeyCode::F4,
        key::KeyCode::F(5) => IttyKeyCode::F5,
        key::KeyCode::F(6) => IttyKeyCode::F6,
        key::KeyCode::F(7) => IttyKeyCode::F7,
        key::KeyCode::F(8) => IttyKeyCode::F8,
        key::KeyCode::F(9) => IttyKeyCode::F9,
        key::KeyCode::F(10) => IttyKeyCode::F10,
        key::KeyCode::F(11) => IttyKeyCode::F11,
        key::KeyCode::F(12) => IttyKeyCode::F12,
        key::KeyCode::Char(ch) => IttyKeyCode::Char(ch),
        _ => return None,
    };

    Some(IttyKey { code, modifiers })
}

/// Convert backend cursor state into Canopy's cursor model.
fn cursor_from_state(state: &TerminalState) -> Option<cursor::Cursor> {
    let (row, col) = state.cursor.grid_pos?;
    if !state.cursor.visible_in_viewport {
        return None;
    }

    let shape = match state.cursor.shape.as_str() {
        "Underline" => cursor::CursorShape::Underscore,
        "Beam" => cursor::CursorShape::Line,
        _ => cursor::CursorShape::Block,
    };
    Some(cursor::Cursor {
        location: geom::Point {
            x: col as u32,
            y: row as u32,
        },
        shape,
        blink: false,
    })
}

/// Render one styled run from the backend snapshot into Canopy cells.
fn render_run(
    rndr: &mut Render,
    origin: geom::Point,
    row_idx: usize,
    run: &StyledRunPublic,
    selection: Option<ViewportSelection>,
    default_bg: Color,
) -> Result<()> {
    let mut col = run.start_col;
    for ch in run.text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        let mut fg = Color::Rgb {
            r: run.fg.r(),
            g: run.fg.g(),
            b: run.fg.b(),
        };
        let mut bg = run.bg.map_or(default_bg, |color| Color::Rgb {
            r: color.r(),
            g: color.g(),
            b: color.b(),
        });
        if selection_contains(selection, row_idx, col) {
            mem::swap(&mut fg, &mut bg);
        }

        let attrs = AttrSet {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            crossedout: run.strikethrough,
            ..AttrSet::default()
        };

        let style = ResolvedStyle::new(fg, bg, attrs);
        rndr.put_cell(
            style,
            geom::Point {
                x: origin.x.saturating_add(col as u32),
                y: origin.y.saturating_add(row_idx as u32),
            },
            ch,
        )?;
        col += width;
    }
    Ok(())
}

/// Return true when a viewport selection contains the given cell.
fn selection_contains(selection: Option<ViewportSelection>, row: usize, col: usize) -> bool {
    let Some(selection) = selection else {
        return false;
    };
    if selection.block {
        let start_row = selection.start_row.min(selection.end_row);
        let end_row = selection.start_row.max(selection.end_row);
        let start_col = selection.start_col.min(selection.end_col);
        let end_col = selection.start_col.max(selection.end_col);
        return (start_row..=end_row).contains(&row) && (start_col..=end_col).contains(&col);
    }
    if row < selection.start_row || row > selection.end_row {
        return false;
    }
    if selection.start_row == selection.end_row {
        let start_col = selection.start_col.min(selection.end_col);
        let end_col = selection.start_col.max(selection.end_col);
        return (start_col..=end_col).contains(&col);
    }
    if row == selection.start_row {
        return col >= selection.start_col;
    }
    if row == selection.end_row {
        return col <= selection.end_col;
    }
    true
}

/// Encode a Canopy mouse event into terminal escape sequences.
fn encode_mouse(event: &mouse::MouseEvent, state: &TerminalState) -> Option<Vec<u8>> {
    let cols = state.cols.max(1) as u32;
    let rows = state.lines.max(1) as u32;
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

    if state.modes.mouse_sgr {
        let suffix = if event.action == mouse::Action::Up {
            'm'
        } else {
            'M'
        };
        let sequence = format!("\x1b[<{cb};{x};{y}{suffix}");
        return Some(sequence.into_bytes());
    }

    let cb = (cb + 32).min(255) as u8;
    let x = (x + 32).min(255) as u8;
    let y = (y + 32).min(255) as u8;
    Some(vec![0x1b, b'[', b'M', cb, x, y])
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };

    use canopy::event::{key, mouse};
    use itty_script::{
        RunMetrics, ScriptExecPolicy, SharedEngineFactory, TermModuleBuilder, run_source,
    };

    use super::*;

    fn mounted_terminal() -> Terminal {
        let mut terminal = Terminal::new(TerminalConfig::default());
        terminal
            .mount_session()
            .expect("mount itty-backed terminal");
        terminal
    }

    #[test]
    fn maps_shift_backtab_to_shift_tab() {
        let key = key::Shift + key::KeyCode::BackTab;
        let mapped = map_key(key).expect("mapped");
        assert_eq!(mapped.code, IttyKeyCode::Tab);
        assert!(mapped.modifiers.contains(IttyModifiers::SHIFT));
    }

    #[test]
    fn double_click_selects_word() {
        let mut terminal = mounted_terminal();
        terminal
            .session_mut()
            .expect("session")
            .set_visible_lines(&["hello world".to_string()])
            .expect("seed lines");

        let point = geom::Point { x: 1, y: 0 };
        assert!(terminal.handle_selection_start(point));
        assert!(terminal.handle_selection_end());
        assert!(terminal.handle_selection_start(point));

        let selected = terminal
            .session()
            .and_then(Session::copy_selection)
            .expect("word selection");
        assert_eq!(selected, "hello");
    }

    #[test]
    fn focus_events_enqueue_focus_reports() {
        let terminal = mounted_terminal();
        terminal.queue_input(Vec::new());
        terminal.sync_focus(true);
        terminal.sync_focus(false);
    }

    #[test]
    #[ignore = "unstable under full-workspace nextest runs; verify with a targeted cargo test"]
    fn itty_script_can_drive_attached_handle() {
        let mut terminal = mounted_terminal();
        let handle = terminal.driver_handle().expect("driver handle");
        let script_done = Arc::new(AtomicBool::new(false));
        let script_done_flag = Arc::clone(&script_done);

        let runner = thread::spawn(move || {
            let runtime = Arc::new(
                Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("script runtime"),
            );
            let metrics = Arc::new(RunMetrics::new());
            let builder = TermModuleBuilder::new(&runtime, &handle, &metrics);
            let setup: SharedEngineFactory =
                Arc::new(|lua, term_builder| term_builder.install_lua(lua));
            let result = run_source(
                builder.context(),
                &setup,
                ScriptExecPolicy::default(),
                None,
                "attached_test.luau",
                "local term = open()\nterm:paste('echo canopy\\r')\nterm:wait_text('canopy')\n",
                BTreeMap::new(),
            );
            script_done_flag.store(true, Ordering::Relaxed);
            result
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while !script_done.load(Ordering::Relaxed) && Instant::now() < deadline {
            terminal.poll_driver();
            thread::sleep(Duration::from_millis(10));
        }

        assert!(
            script_done.load(Ordering::Relaxed),
            "script did not finish in time"
        );
        runner
            .join()
            .expect("script thread")
            .expect("script succeeds");

        terminal.queue_input(b"exit\r".to_vec());
        let shutdown_deadline = Instant::now() + Duration::from_secs(5);
        while !terminal.session().is_some_and(Session::child_exited)
            && Instant::now() < shutdown_deadline
        {
            terminal.poll_driver();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            terminal.session().is_some_and(Session::child_exited),
            "attached shell did not exit in time"
        );
    }

    #[test]
    fn mouse_encoding_uses_sgr_when_requested() {
        let mut terminal = mounted_terminal();
        let mut state = terminal.session_mut().expect("session").state();
        state.modes.mouse_report_click = true;
        state.modes.mouse_drag = false;
        state.modes.mouse_motion = false;
        state.modes.mouse_sgr = true;
        let event = mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location: geom::Point { x: 4, y: 6 },
        };

        let encoded = encode_mouse(&event, &state).expect("mouse bytes");
        assert_eq!(encoded, b"\x1b[<0;5;7M");
    }
}
