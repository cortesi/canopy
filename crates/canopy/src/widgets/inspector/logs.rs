use std::{
    io::{Result as IoResult, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing_subscriber::fmt;

use crate::{
    Canopy, Context, Loader, ViewContext, command, derive_commands,
    error::Result,
    geom::{Expanse, Point, Rect},
    layout::{CanvasContext, MeasureConstraints, Measurement, Size},
    render::Render,
    state::NodeName,
    widget::Widget,
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

    fn render(
        &mut self,
        rndr: &mut Render,
        area: Rect,
        selected: bool,
        offset: Point,
        full_size: Expanse,
    ) -> Result<()> {
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        let offset_x = offset.x;
        let offset_y = offset.y as usize;
        let text_width = full_size.w.saturating_sub(2).max(1) as usize;
        let lines = textwrap::wrap(&self.text, text_width);

        if offset_x == 0 && area.w >= 1 {
            let status = Rect::new(area.tl.x, area.tl.y, 1, area.h);
            if selected {
                rndr.fill("blue", status, '\u{2588}')?;
            } else {
                rndr.fill("", status, ' ')?;
            }
        }

        if offset_x <= 1 {
            let spacer_x = area.tl.x.saturating_add(1u32.saturating_sub(offset_x));
            if spacer_x < area.tl.x.saturating_add(area.w) {
                let spacer = Rect::new(spacer_x, area.tl.y, 1, area.h);
                rndr.fill("", spacer, ' ')?;
            }
        }

        let text_offset_x = offset_x.saturating_sub(2);
        let text_start_x = if offset_x >= 2 {
            area.tl.x
        } else {
            area.tl.x.saturating_add(2u32.saturating_sub(offset_x))
        };
        let text_visible_width = area
            .w
            .saturating_sub(text_start_x.saturating_sub(area.tl.x))
            .max(1);

        if text_visible_width == 0 {
            return Ok(());
        }

        for (idx, line) in lines
            .iter()
            .enumerate()
            .skip(offset_y)
            .take(area.h as usize)
        {
            let start_char = text_offset_x as usize;
            let start_byte = line
                .char_indices()
                .nth(start_char)
                .map(|(i, _)| i)
                .unwrap_or(line.len());
            let out = &line[start_byte..];
            let line_rect = Rect::new(
                text_start_x,
                area.tl.y.saturating_add((idx - offset_y) as u32),
                text_visible_width,
                1,
            );
            rndr.text("text", line_rect.line(0), out)?;
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
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        self.list.render(rndr, ctx)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        self.list.measure(c)
    }

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
        self.list.canvas(view, ctx)
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
    /// Scroll the view down by one line.
    pub fn scroll_down(&mut self, c: &mut dyn Context) {
        self.list.scroll_down(c);
    }

    #[command]
    /// Scroll the view up by one line.
    pub fn scroll_up(&mut self, c: &mut dyn Context) {
        self.list.scroll_up(c);
    }

    #[command]
    /// Scroll the view left by one line.
    pub fn scroll_left(&mut self, c: &mut dyn Context) {
        self.list.scroll_left(c);
    }

    #[command]
    /// Scroll the view right by one line.
    pub fn scroll_right(&mut self, c: &mut dyn Context) {
        self.list.scroll_right(c);
    }

    #[command]
    /// Scroll the view down by one page.
    pub fn page_down(&mut self, c: &mut dyn Context) {
        self.list.page_down(c);
    }

    #[command]
    /// Scroll the view up by one page.
    pub fn page_up(&mut self, c: &mut dyn Context) {
        self.list.page_up(c);
    }
}

impl Loader for Logs {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
    }
}
