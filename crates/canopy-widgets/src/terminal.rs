use std::{path::PathBuf, sync::Arc, time::Duration};

use canopy::{
    Context, EventOutcome, ViewContext, Widget, cursor, derive_commands,
    error::{Error, Result},
    event::{self, key, mouse},
    geom::{self, Size},
    layout::{CanvasContext, MeasureConstraints, Measurement},
    render::Render,
    rgb,
    state::NodeName,
    style::{AttrSet, Color, ResolvedStyle, Style},
    text,
};
use itty_core::{
    SelectionSnapshot, SelectionSpec, Session,
    clipboard::{ClipboardHandler, SystemClipboard},
    config::{EguiTTYConfig, EguiTTYConfigBuilder, Hex, PaletteConfig, PaletteKind, PaletteMeta},
    inspect::{StyledRunPublic, TerminalState},
    key::{Key as IttyKey, KeyCode as IttyKeyCode, Modifiers as IttyModifiers},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::click::ClickTracker;

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

/// Default terminal background color.
const DEFAULT_BACKGROUND: Color = rgb!("#000000");

/// Build the default terminal color palette.
fn default_palette() -> PaletteConfig {
    PaletteConfig {
        normal: [
            canopy_hex(rgb!("#000000")),
            canopy_hex(rgb!("#cc0000")),
            canopy_hex(rgb!("#4e9a06")),
            canopy_hex(rgb!("#c4a000")),
            canopy_hex(rgb!("#3465a4")),
            canopy_hex(rgb!("#75507b")),
            canopy_hex(rgb!("#06989a")),
            canopy_hex(rgb!("#d3d7cf")),
        ],
        bright: [
            canopy_hex(rgb!("#555753")),
            canopy_hex(rgb!("#ef2929")),
            canopy_hex(rgb!("#8ae234")),
            canopy_hex(rgb!("#fce94f")),
            canopy_hex(rgb!("#729fcf")),
            canopy_hex(rgb!("#ad7fa8")),
            canopy_hex(rgb!("#34e2e2")),
            canopy_hex(rgb!("#eeeeec")),
        ],
        dim: None,
        foreground: canopy_hex(rgb!("#eeeeec")),
        background: canopy_hex(DEFAULT_BACKGROUND),
        cursor: canopy_hex(rgb!("#ffffff")),
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

/// Terminal widget configuration.
#[derive(Default)]
pub struct TerminalConfig {
    /// Optional command argv to run instead of the default shell.
    command: Option<Vec<String>>,
    /// Working directory for the terminal process.
    cwd: Option<PathBuf>,
}

impl TerminalConfig {
    /// Construct a default terminal configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure the command argv to run instead of the default shell.
    pub fn with_command<I, S>(mut self, command: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.command = Some(command.into_iter().map(Into::into).collect());
        self
    }

    /// Configure the working directory for the terminal process.
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

/// Terminal widget backed by `itty`.
pub struct Terminal {
    /// User-provided configuration.
    config: TerminalConfig,
    /// Backend terminal session.
    session: Option<Session>,
    /// Most recent terminal size.
    last_size: TerminalSize,
    /// Cached cursor for rendering.
    cursor: Option<cursor::Cursor>,
    /// Whether a selection drag is active.
    selection_active: bool,
    /// Selection anchor in viewport coordinates.
    selection_anchor: Option<geom::Point>,
    /// Multi-click tracking state.
    last_click: ClickTracker,
}

#[derive_commands]
impl Terminal {
    /// Construct a new terminal widget with the provided configuration.
    pub fn new(config: TerminalConfig) -> Self {
        Self {
            config,
            session: None,
            last_size: TerminalSize {
                columns: DEFAULT_COLUMNS,
                rows: DEFAULT_LINES,
            },
            cursor: None,
            selection_active: false,
            selection_anchor: None,
            last_click: ClickTracker::new(Duration::from_millis(DOUBLE_CLICK_MS)),
        }
    }

    /// Lazily create the backend session.
    fn mount_session(&mut self) -> Result<()> {
        if self.session.is_some() {
            return Ok(());
        }

        let cfg = terminal_config(&self.config, self.last_size);
        let mut session =
            Session::from_config(&cfg).map_err(|error| Error::Internal(error.to_string()))?;
        session.set_clipboard_handler(Arc::new(SystemClipboard));

        self.session = Some(session);
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

    /// Ensure the terminal grid matches the current view.
    fn ensure_size(&mut self, expanse: geom::Size) {
        let size = TerminalSize::from_expanse(expanse);
        if self.last_size == size {
            return;
        }

        self.last_size = size;
        if let Some(session) = self.session.as_mut() {
            drop(session.resize_grid_and_pty(size.columns, size.rows, 1.0));
        }
    }

    /// Return the current backend state snapshot.
    fn state(&self) -> Option<TerminalState> {
        self.session().map(Session::state)
    }

    /// Clear the current viewport selection overlay.
    fn clear_selection(&mut self) {
        if let Some(session) = self.session_mut() {
            session.set_selection(None);
        }
        self.selection_active = false;
        self.selection_anchor = None;
    }

    /// Update the viewport selection overlay between two points.
    fn set_selection(&mut self, start: geom::Point, end: geom::Point) -> bool {
        let Some(state) = self.state() else {
            return false;
        };
        let Some(session) = self.session_mut() else {
            return false;
        };
        let start_line = start.y as i32 - state.display_offset as i32;
        let end_line = end.y as i32 - state.display_offset as i32;

        session.set_selection(Some(SelectionSpec {
            start_line,
            start_col: start.x as usize,
            end_line,
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

    /// Select a single semantic word around the provided viewport point.
    fn select_word(&mut self, point: geom::Point) -> bool {
        let Some(line) = self
            .session()
            .and_then(|session| session.line_text(point.y as usize))
        else {
            return false;
        };

        let mut column = 0;
        let spans: Vec<_> = line
            .graphemes(true)
            .map(|grapheme| {
                let start = column;
                column += text::grapheme_width(grapheme);
                (start, column, grapheme.chars().all(char::is_whitespace))
            })
            .collect();
        let Some(index) = spans
            .iter()
            .position(|&(start, end, _)| start <= point.x as usize && (point.x as usize) < end)
        else {
            return self.set_selection(point, point);
        };
        if spans[index].2 {
            return self.set_selection(point, point);
        }
        let mut first = index;
        while first > 0 && !spans[first - 1].2 {
            first -= 1;
        }
        let mut last = index;
        while last + 1 < spans.len() && !spans[last + 1].2 {
            last += 1;
        }
        let start = spans[first].0;
        let end = spans[last].1.saturating_sub(1);

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

        match self.last_click.count(point) {
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

    /// Send a mouse input sequence to the terminal when mouse reporting is
    /// enabled.
    fn send_mouse_sequence(&self, event: &mouse::MouseEvent, state: &TerminalState) {
        let Some(bytes) = encode_mouse(event, state) else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        drop(session.send_input_bytes(bytes, "mouse"));
    }

    /// Swallow Ctrl+Shift+C so the chord does not reach the PTY.
    fn copy_selection(&self) {
        let Some(text) = self.session().and_then(Session::copy_selection) else {
            return;
        };
        drop(SystemClipboard.set_text(&text));
    }

    /// Send pasted content to the PTY.
    fn handle_paste(&self, content: &str) {
        let Some(session) = self.session() else {
            return;
        };
        drop(session.paste(content));
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

    /// Return the focus report the terminal expects, if it enabled focus
    /// reporting.
    fn focus_report(&self, focused: bool) -> Option<Vec<u8>> {
        let state = self.state()?;
        state.modes.focus_in_out.then(|| {
            if focused {
                b"\x1b[I".to_vec()
            } else {
                b"\x1b[O".to_vec()
            }
        })
    }

    /// Forward a focus change to the terminal as a focus report.
    fn sync_focus(&self, focused: bool) {
        if self.focus_report(focused).is_none() {
            return;
        }
        if let Some(session) = self.session() {
            session.send_focus_report(focused);
        }
    }
}

impl Widget for Terminal {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
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
        let selection = session.selection();
        let child_exited = session.child_exited();
        let child_exit_code = session.child_exit_code().unwrap_or(1);
        let default_bg = DEFAULT_BACKGROUND;
        self.cursor = cursor_from_state(&state);

        for (row_idx, line) in runs.iter().enumerate() {
            for run in line {
                render_run(
                    rndr,
                    view.content_origin(),
                    row_idx,
                    run,
                    selection,
                    state.display_offset,
                    default_bg,
                )?;
            }
        }

        if child_exited {
            let message = format!("Process exited (status {child_exit_code})");
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
                ctx.set_focus(ctx.node_id())?;
                let Some(state) = self.state() else {
                    return Ok(EventOutcome::Ignore);
                };

                let mouse_reporting = state.modes.mouse_report_click
                    || state.modes.mouse_drag
                    || state.modes.mouse_motion;
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
                self.sync_focus(true);
                Ok(EventOutcome::Handle)
            }
            event::Event::FocusLost => {
                self.sync_focus(false);
                Ok(EventOutcome::Handle)
            }
            _ => Ok(EventOutcome::Ignore),
        }
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.wrap()
    }

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        view
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn cursor(&self) -> Option<cursor::Cursor> {
        self.cursor
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
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
    let (r, g, b) = color.rgb();
    Hex::from_rgb(itty_core::Rgb { r, g, b })
}

/// Build an `itty` config from Canopy's terminal config.
fn terminal_config(config: &TerminalConfig, size: TerminalSize) -> EguiTTYConfig {
    let mut builder = EguiTTYConfigBuilder::new()
        .grid_fixed(size.columns, size.rows)
        .scrollback_lines(DEFAULT_SCROLLBACK)
        .kitty_keyboard(true)
        .palette_inline(default_palette());

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
    })
}

/// Render one styled run from the backend snapshot into Canopy cells.
fn render_run(
    rndr: &mut Render,
    origin: geom::Point,
    row_idx: usize,
    run: &StyledRunPublic,
    selection: Option<SelectionSnapshot>,
    display_offset: usize,
    default_bg: Color,
) -> Result<()> {
    // Apply effects once to each variant after the raw selection color swap.
    let base = ResolvedStyle::new(
        Color::Rgb {
            r: run.fg.r(),
            g: run.fg.g(),
            b: run.fg.b(),
        },
        run.bg.map_or(default_bg, |color| Color::Rgb {
            r: color.r(),
            g: color.g(),
            b: color.b(),
        }),
        AttrSet {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            crossedout: run.strikethrough,
            ..AttrSet::default()
        },
    );

    let selected = rndr.apply_effects(Style {
        fg: base.bg.into(),
        bg: base.fg.into(),
        attrs: base.attrs,
    });
    let base = rndr.apply_effects(Style {
        fg: base.fg.into(),
        bg: base.bg.into(),
        attrs: base.attrs,
    });

    let bounds = geom::Rect::new(
        origin.x.saturating_add(run.start_col as u32),
        origin.y.saturating_add(row_idx as u32),
        run.end_col.saturating_sub(run.start_col) as u32,
        1,
    );
    let mut col = run.start_col;
    for grapheme in run.text.graphemes(true) {
        let width = text::grapheme_width(grapheme);
        let style = if selection_contains(selection, row_idx, col, display_offset) {
            &selected
        } else {
            &base
        };
        let point = geom::Point {
            x: origin.x.saturating_add(col as u32),
            y: origin.y.saturating_add(row_idx as u32),
        };
        rndr.put_grapheme(style.resolve_at(bounds, point), point, grapheme)?;
        col += width;
    }
    Ok(())
}

/// Return true when a viewport selection contains the given cell.
fn selection_contains(
    selection: Option<SelectionSnapshot>,
    row: usize,
    col: usize,
    display_offset: usize,
) -> bool {
    let Some(selection) = selection else {
        return false;
    };
    let row = row as i32 - display_offset as i32;
    if selection.block {
        let start_row = selection.start_line.min(selection.end_line);
        let end_row = selection.start_line.max(selection.end_line);
        let start_col = selection.start_col.min(selection.end_col);
        let end_col = selection.start_col.max(selection.end_col);
        return (start_row..=end_row).contains(&row) && (start_col..=end_col).contains(&col);
    }
    if row < selection.start_line || row > selection.end_line {
        return false;
    }
    if selection.start_line == selection.end_line {
        let start_col = selection.start_col.min(selection.end_col);
        let end_col = selection.start_col.max(selection.end_col);
        return (start_col..=end_col).contains(&col);
    }
    if row == selection.start_line {
        return col >= selection.start_col;
    }
    if row == selection.end_line {
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

    if event.action == mouse::Action::Up && !state.modes.mouse_sgr {
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
    use std::sync::Arc;

    use canopy::{
        ContextExt, TermBuf,
        event::{key, mouse},
        layout::Layout,
        style::{
            GradientSpec, GradientStop, Paint, StyleManager, StyleMap,
            effects::{self, Effect, StyleEffect},
        },
        testing::harness::Harness,
    };
    use itty_core::{colors::Rgba8, palette::builtin};

    use super::*;

    fn stream_terminal() -> (Terminal, itty_core::StreamInputReceiver) {
        let (session, receiver) =
            Session::new_stream(DEFAULT_COLUMNS, DEFAULT_LINES, builtin::one_dark())
                .expect("stream terminal");
        let mut terminal = Terminal::new(TerminalConfig::default());
        terminal.session = Some(session);
        (terminal, receiver)
    }

    #[test]
    fn renders_terminal_graphemes_with_their_cell_widths() {
        let base_style = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let mut buf = TermBuf::new(geom::Size::new(6, 1), '\0', base_style).expect("buffer");
        let stylemap = StyleMap::new();
        let mut styles = StyleManager::new();
        let run = StyledRunPublic {
            text: "A⚡e\u{301}B".to_string(),
            fg: Rgba8 {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            bg: None,
            italic: false,
            underline: false,
            strikethrough: false,
            bold: false,
            hyperlink: None,
            start_col: 0,
            end_col: 5,
        };

        {
            let mut rndr = Render::new(
                &stylemap,
                &mut styles,
                &mut buf,
                geom::Rect::new(0, 0, 6, 1),
                geom::Point::default(),
            );
            render_run(
                &mut rndr,
                geom::Point::default(),
                0,
                &run,
                None,
                0,
                Color::Black,
            )
            .expect("render terminal run");
        }

        assert_eq!(buf.rows()[0], ["A", "⚡", "", "e\u{301}", "B", " "]);
    }

    /// Exercise the run renderer as a child that inherits its parent's effects.
    struct EffectRun;

    impl Widget for EffectRun {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            let run = StyledRunPublic {
                text: "AB".to_string(),
                fg: Rgba8 {
                    r: 200,
                    g: 100,
                    b: 40,
                    a: 255,
                },
                bg: Some(Rgba8 {
                    r: 40,
                    g: 80,
                    b: 120,
                    a: 255,
                }),
                italic: true,
                underline: false,
                strikethrough: false,
                bold: false,
                hyperlink: None,
                start_col: 0,
                end_col: 2,
            };
            render_run(
                render,
                geom::Point::default(),
                0,
                &run,
                Some(SelectionSnapshot {
                    start_line: 0,
                    start_col: 1,
                    end_line: 0,
                    end_col: 1,
                    block: false,
                }),
                0,
                Color::Black,
            )
        }
    }

    fn render_effect_run(effects: Vec<Effect>) -> Result<TermBuf> {
        let mut canopy = canopy::Canopy::new();
        canopy.with_root_context(|ctx| {
            ctx.set_layout(Layout::fill())?;
            let _ = ctx.add_child(EffectRun)?;
            for effect in effects {
                ctx.push_effect(ctx.node_id(), effect)?;
            }
            Ok(())
        })?;
        let mut harness = Harness::from_canopy(canopy, geom::Size::new(2, 1))?;
        harness.render()?;
        Ok(harness.buf().clone())
    }

    #[test]
    fn terminal_runs_inherit_effects_once_for_both_selection_variants() -> Result<()> {
        for dimmed in [false, true] {
            let effects = if dimmed {
                vec![effects::brightness(0.5), effects::bold()]
            } else {
                vec![]
            };
            let buf = render_effect_run(effects)?;
            let (fg, bg) = if dimmed {
                (
                    Color::Rgb {
                        r: 100,
                        g: 50,
                        b: 20,
                    },
                    Color::Rgb {
                        r: 20,
                        g: 40,
                        b: 60,
                    },
                )
            } else {
                (
                    Color::Rgb {
                        r: 200,
                        g: 100,
                        b: 40,
                    },
                    Color::Rgb {
                        r: 40,
                        g: 80,
                        b: 120,
                    },
                )
            };
            let attrs = AttrSet {
                bold: dimmed,
                italic: true,
                ..AttrSet::default()
            };
            let normal = buf.get(geom::Point { x: 0, y: 0 }).expect("normal cell");
            let selected = buf.get(geom::Point { x: 1, y: 0 }).expect("selected cell");
            assert_eq!(normal.ch, 'A');
            assert_eq!(selected.ch, 'B');
            assert_eq!(normal.style, ResolvedStyle::new(fg, bg, attrs));
            assert_eq!(selected.style, ResolvedStyle::new(bg, fg, attrs));
        }
        Ok(())
    }

    #[derive(Debug)]
    struct ForegroundGradient;

    impl StyleEffect for ForegroundGradient {
        fn apply(&self, mut style: Style) -> Style {
            style.fg = Paint::gradient(GradientSpec::with_stops(
                0.0,
                vec![GradientStop::new(0.0, Color::Blue)],
            ));
            style
        }
    }

    #[test]
    fn terminal_selection_swaps_raw_colors_before_custom_effects() -> Result<()> {
        let buf = render_effect_run(vec![Arc::new(ForegroundGradient)])?;
        let normal = buf
            .get(geom::Point { x: 0, y: 0 })
            .expect("normal cell")
            .style;
        let selected = buf
            .get(geom::Point { x: 1, y: 0 })
            .expect("selected cell")
            .style;
        assert_eq!(normal.fg, Color::Blue);
        assert_eq!(selected.fg, Color::Blue);
        assert_eq!(
            normal.bg,
            Color::Rgb {
                r: 40,
                g: 80,
                b: 120
            }
        );
        assert_eq!(
            selected.bg,
            Color::Rgb {
                r: 200,
                g: 100,
                b: 40
            }
        );
        Ok(())
    }

    #[test]
    fn word_selection_uses_cell_columns_for_wide_and_combining_graphemes() {
        let (mut terminal, _receiver) = stream_terminal();
        terminal
            .session_mut()
            .expect("session")
            .set_visible_lines(&["界 e\u{301}x word".to_string()])
            .expect("seed lines");
        for (column, expected) in [
            (0, "界"),
            (1, "界"),
            (2, " "),
            (3, "e\u{301}x"),
            (4, "e\u{301}x"),
            (5, " "),
            (6, "word"),
            (9, "word"),
        ] {
            assert!(terminal.select_word(geom::Point { x: column, y: 0 }));
            assert_eq!(
                terminal
                    .session()
                    .and_then(Session::copy_selection)
                    .as_deref(),
                Some(expected),
                "column {column}"
            );
        }
    }

    #[test]
    fn mouse_button_releases_preserve_sgr_buttons_and_legacy_release_code() {
        let (terminal, _receiver) = stream_terminal();
        let mut state = terminal.state().expect("state");
        for sgr in [false, true] {
            state.modes.mouse_sgr = sgr;
            for (button, button_code) in [
                (mouse::Button::Left, 0),
                (mouse::Button::Middle, 1),
                (mouse::Button::Right, 2),
            ] {
                for action in [mouse::Action::Down, mouse::Action::Up] {
                    for (modifiers, modifier_code) in [
                        (key::Empty, 0),
                        (key::Shift, 4),
                        (key::Alt, 8),
                        (key::Ctrl, 16),
                        (key::Shift + key::Alt + key::Ctrl, 28),
                    ] {
                        let event = mouse::MouseEvent {
                            action,
                            button,
                            modifiers,
                            location: geom::Point { x: 4, y: 6 },
                        };
                        let code = if action == mouse::Action::Up && !sgr {
                            3
                        } else {
                            button_code
                        } + modifier_code;
                        let expected = if sgr {
                            let suffix = if action == mouse::Action::Up {
                                'm'
                            } else {
                                'M'
                            };
                            format!("\x1b[<{code};5;7{suffix}").into_bytes()
                        } else {
                            vec![0x1b, b'[', b'M', code + 32, 37, 39]
                        };
                        assert_eq!(
                            encode_mouse(&event, &state),
                            Some(expected),
                            "sgr={sgr}, {button:?}, {action:?}, {modifiers:?}"
                        );
                    }
                }
            }
        }
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
        let (mut terminal, _receiver) = stream_terminal();
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
    fn focus_reports_follow_the_terminal_mode() {
        let (terminal, _receiver) = stream_terminal();
        assert_eq!(terminal.focus_report(true), None);
        assert_eq!(terminal.focus_report(false), None);
        let session = terminal.session().expect("session");
        session
            .feed_stream_output(b"\x1b[?1004h")
            .expect("enable focus reports");
        assert_eq!(terminal.focus_report(true), Some(b"\x1b[I".to_vec()));
        assert_eq!(terminal.focus_report(false), Some(b"\x1b[O".to_vec()));
        session
            .feed_stream_output(b"\x1b[?1004l")
            .expect("disable focus reports");
        assert_eq!(terminal.focus_report(true), None);
        assert_eq!(terminal.focus_report(false), None);
    }

    #[test]
    fn mouse_encoding_uses_sgr_when_requested() {
        let (mut terminal, _receiver) = stream_terminal();
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
