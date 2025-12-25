use std::{
    io::{Result as IoResult, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing_subscriber::fmt;

use crate::{
    Canopy, Context, Loader, ViewContext, command, derive_commands,
    error::Result,
    event::Event,
    geom::{Expanse, Rect},
    layout::{AvailableSpace, Size},
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
    widgets::list::{List, ListItem},
};

/// List item for a single log entry.
pub(super) struct LogItem {
    /// Text display.
    text: String,
}

impl LogItem {
    /// Construct a log item from text.
    fn new(txt: &str) -> Self {
        Self {
            text: txt.to_string(),
        }
    }
}

impl ListItem for LogItem {
    fn measure(&self, available_width: u32) -> Expanse {
        let text_width = available_width.saturating_sub(2).max(1) as usize;
        let lines = textwrap::wrap(&self.text, text_width);
        Expanse::new(available_width.max(1), lines.len() as u32)
    }

    fn render(&mut self, rndr: &mut Render, area: Rect, selected: bool) -> Result<()> {
        let status = Rect::new(area.tl.x, area.tl.y, 1, area.h);
        if selected {
            rndr.fill("blue", status, '\u{2588}')?;
        } else {
            rndr.fill("", status, ' ')?;
        }

        if area.w < 2 {
            return Ok(());
        }

        let spacer = Rect::new(area.tl.x + 1, area.tl.y, 1, area.h);
        rndr.fill("", spacer, ' ')?;

        let text_rect = Rect::new(area.tl.x + 2, area.tl.y, area.w - 2, area.h);
        let text_width = text_rect.w.max(1) as usize;
        let lines = textwrap::wrap(&self.text, text_width);
        for (idx, line) in lines.iter().enumerate().take(text_rect.h as usize) {
            rndr.text("text", text_rect.line(idx as u32), line)?;
        }

        Ok(())
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
    /// List of log items.
    list: List<LogItem>,
    /// Whether logging is initialized.
    started: bool,
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Widget for Logs {
    fn render(&mut self, rndr: &mut Render, area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        self.list.render(rndr, area, ctx)
    }

    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        self.list.measure(known_dimensions, available_space)
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, _c: &mut dyn Context) -> Option<Duration> {
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

        self.flush_buffer();
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
            list: List::new(vec![]),
            started: false,
            buf: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Drain buffered log lines into the list.
    fn flush_buffer(&mut self) {
        let buf = self.buf.clone();
        let mut b = buf.lock().unwrap();
        let vals = b.drain(..);
        for i in vals {
            self.list.append(LogItem::new(&i));
        }
    }

    #[command(ignore_result)]
    /// Clear all items.
    pub fn clear(&mut self) -> Vec<LogItem> {
        self.list.clear()
    }

    #[command(ignore_result)]
    /// Delete the currently selected item.
    pub fn delete_selected(&mut self, c: &mut dyn Context) -> Option<LogItem> {
        self.list.delete_selected(c)
    }

    #[command]
    /// Move selection to the first item.
    pub fn select_first(&mut self, c: &mut dyn Context) {
        self.list.select_first(c);
    }

    #[command]
    /// Move selection to the last item.
    pub fn select_last(&mut self, c: &mut dyn Context) {
        self.list.select_last(c);
    }

    #[command]
    /// Move selection to the next item.
    pub fn select_next(&mut self, c: &mut dyn Context) {
        self.list.select_next(c);
    }

    #[command]
    /// Move selection to the previous item.
    pub fn select_prev(&mut self, c: &mut dyn Context) {
        self.list.select_prev(c);
    }

    #[command]
    /// Scroll the viewport down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.list.scroll_down(c);
    }

    #[command]
    /// Scroll the viewport up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.list.scroll_up(c);
    }

    #[command]
    /// Scroll the viewport left by one line.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.list.scroll_left(c);
    }

    #[command]
    /// Scroll the viewport right by one line.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.list.scroll_right(c);
    }

    #[command]
    /// Scroll the viewport down by one page.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.list.page_down(c);
    }

    #[command]
    /// Scroll the viewport up by one page.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.list.page_up(c);
    }
}

impl Loader for Logs {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
    }
}
