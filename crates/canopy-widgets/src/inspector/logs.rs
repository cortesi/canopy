//! Log panel for the inspector widget.

use std::{
    io::{Result as IoResult, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use canopy::{
    Canopy, Context, Loader, ReadContext, Widget, command,
    commands::{ScrollDirection, VerticalDirection},
    derive_commands,
    error::{Error, Result},
    geom::Rect,
    key,
    layout::{CanvasContext, Constraint, Layout, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
};
use tracing_subscriber::fmt;

use crate::{List, Selectable};

key!(ListSlot: List<LogEntry>);

/// Widget for displaying a single log entry.
pub struct LogEntry {
    /// Text content.
    text: String,
    /// Selection state.
    selected: bool,
}

impl Selectable for LogEntry {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

#[derive_commands]
impl LogEntry {
    /// Construct a log entry from text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            selected: false,
        }
    }
}

impl Widget for LogEntry {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let available_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => 80,
        };
        let text_width = available_width.saturating_sub(2).max(1) as usize;
        let lines = textwrap::wrap(&self.text, text_width);
        c.clamp(Size::new(available_width, lines.len() as u32))
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ReadContext) -> Result<()> {
        let view = ctx.view();

        if view.is_zero() {
            return Ok(());
        }

        // Wrap text based on view width, then render at canvas coordinates
        let text_width = view.content.w.saturating_sub(2).max(1) as usize;
        let lines: Vec<_> = textwrap::wrap(&self.text, text_width).into_iter().collect();
        let height = lines.len().max(1) as u32;

        // Render in canvas coordinates (0,0 is top-left of content).
        // Column 0: Selection indicator (when selected)
        if self.selected {
            let indicator_rect = Rect::new(0, 0, 1, height);
            rndr.fill("list/selected", indicator_rect, '\u{2588}')?;
        }

        // Column 1: Spacer
        let spacer = Rect::new(1, 0, 1, height);
        rndr.fill("", spacer, ' ')?;

        // Text content starts at column 2
        for (idx, line) in lines.iter().enumerate() {
            let line_rect = Rect::new(2, idx as u32, line.len() as u32, 1);
            rndr.text("text", line_rect.line(0), line)?;
        }

        Ok(())
    }

    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("log_entry")
    }
}

/// Log writer that appends to a shared buffer.
struct LogWriter {
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.buf
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(buf).to_string().trim().to_string());
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// Inspector log panel.
pub struct Logs {
    /// Whether logging is initialized.
    started: bool,
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Widget for Logs {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        rndr.push_layer("logs");
        Ok(())
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let available_width = match c.width {
            Constraint::Exact(n) | Constraint::AtMost(n) => n,
            Constraint::Unbounded => 80,
        };
        c.clamp(Size::new(available_width, 10))
    }

    fn canvas(&self, view: Size<u32>, _ctx: &CanvasContext) -> Size<u32> {
        view
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c).ok()?;

        if !self.started {
            let format = fmt::format()
                .with_level(true)
                .with_line_number(true)
                .with_ansi(false)
                .without_time()
                .compact();

            let buf = self.buf.clone();
            tracing_subscriber::fmt()
                .with_writer(move || -> LogWriter { LogWriter { buf: buf.clone() } })
                .event_format(format)
                .init();
            self.started = true;
        }

        self.flush_buffer(c).ok();
        Some(Duration::from_millis(100))
    }

    fn name(&self) -> NodeName {
        NodeName::convert("logs")
    }
}

#[derive_commands]
impl Logs {
    /// Construct a log panel.
    pub fn new() -> Self {
        Self {
            started: false,
            buf: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Ensure the list widget is mounted.
    fn ensure_tree(&self, c: &mut dyn Context) -> Result<()> {
        if c.has_child::<ListSlot>() {
            return Ok(());
        }

        let list_id = c.add_keyed::<ListSlot>(List::<LogEntry>::new())?;
        c.set_layout_of(list_id, Layout::fill())?;
        Ok(())
    }

    /// Execute a closure with the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnMut(&mut List<LogEntry>, &mut dyn Context) -> Result<R>,
    {
        if !c.has_child::<ListSlot>() {
            return Err(Error::Internal("logs list not initialized".into()));
        }
        c.with_child::<ListSlot, _>(f)
    }

    /// Drain buffered log lines into the list.
    fn flush_buffer(&self, c: &mut dyn Context) -> Result<()> {
        let buf = self.buf.clone();
        let mut b = buf.lock().unwrap();
        let vals: Vec<String> = b.drain(..).collect();
        drop(b);

        if !c.has_child::<ListSlot>() {
            return Ok(());
        }

        for line in vals {
            let mut entry = Some(LogEntry::new(line));
            c.with_child::<ListSlot, _>(|list, ctx| {
                if let Some(e) = entry.take() {
                    list.append(ctx, e)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    #[command]
    /// Clear all items.
    pub fn clear(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.clear(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Delete the currently selected item.
    pub fn delete_selected(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.delete_selected(ctx)?;
            Ok(())
        })
    }

    #[command]
    /// Move selection to the first item.
    pub fn select_first(&self, c: &mut dyn Context) {
        drop(self.with_list(c, |list, ctx| {
            list.select_first(ctx);
            Ok(())
        }));
    }

    #[command]
    /// Move selection to the last item.
    pub fn select_last(&self, c: &mut dyn Context) {
        drop(self.with_list(c, |list, ctx| {
            list.select_last(ctx);
            Ok(())
        }));
    }

    #[command]
    /// Move selection by a signed offset.
    pub fn select_by(&self, c: &mut dyn Context, delta: i32) {
        drop(self.with_list(c, |list, ctx| {
            list.select_by(ctx, delta);
            Ok(())
        }));
    }

    /// Scroll the view by one line in the specified direction.
    pub fn scroll(&self, c: &mut dyn Context, dir: ScrollDirection) {
        drop(self.with_list(c, |list, ctx| {
            list.scroll(ctx, dir);
            Ok(())
        }));
    }

    /// Move selection by one page in the specified direction.
    pub fn page(&self, c: &mut dyn Context, dir: VerticalDirection) {
        drop(self.with_list(c, |list, ctx| {
            list.page(ctx, dir);
            Ok(())
        }));
    }

    #[command]
    /// Scroll up by one line.
    pub fn scroll_up(&self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Up);
    }

    #[command]
    /// Scroll down by one line.
    pub fn scroll_down(&self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Down);
    }

    #[command]
    /// Scroll left by one column.
    pub fn scroll_left(&self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Left);
    }

    #[command]
    /// Scroll right by one column.
    pub fn scroll_right(&self, c: &mut dyn Context) {
        self.scroll(c, ScrollDirection::Right);
    }

    #[command]
    /// Page up by one screen.
    pub fn page_up(&self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Up);
    }

    #[command]
    /// Page down by one screen.
    pub fn page_down(&self, c: &mut dyn Context) {
        self.page(c, VerticalDirection::Down);
    }
}

impl Default for Logs {
    fn default() -> Self {
        Self::new()
    }
}

impl Loader for Logs {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        Ok(())
    }
}
