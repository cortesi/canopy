//! Log panel for the inspector widget.

use std::{
    io::{Result as IoResult, Write},
    mem,
    result::Result as StdResult,
    sync::{Arc, Mutex},
    time::Duration,
};

use canopy::{
    Canopy, Context, FocusDirection, Loader, ViewContext, Widget, derive_commands,
    error::Result,
    geom::Size,
    layout::{CanvasContext, Constraint, Layout, MeasureConstraints, Measurement},
    render::Render,
    state::NodeName,
};
use tracing_subscriber::fmt;

use crate::{List, Text};

canopy::slot!(ListSlot: List<Text>);

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
            .push(String::from_utf8_lossy(buf).trim().to_string());
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}

/// Whether this panel owns the process-wide tracing subscriber.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InstallState {
    /// Installation has not been attempted yet.
    Unattempted,
    /// This panel installed the subscriber and receives events.
    Active,
    /// Another subscriber was already installed, with this reason.
    Unavailable(String),
}

/// Classify the outcome of one subscriber installation attempt.
fn after_install(result: StdResult<(), String>) -> InstallState {
    match result {
        Ok(()) => InstallState::Active,
        Err(message) => InstallState::Unavailable(message),
    }
}

/// Inspector log panel.
pub struct Logs {
    /// Whether this panel owns the tracing subscriber.
    install: InstallState,
    /// Shared log buffer.
    buf: Arc<Mutex<Vec<String>>>,
}

impl Widget for Logs {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
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

    fn canvas(&self, view: Size, _ctx: &CanvasContext) -> Size {
        view
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        c.add_slot::<ListSlot>(List::<Text>::new().with_selection_indicator(
            "list/selected",
            "█ ",
            true,
        ))?;
        Ok(())
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        if self.install == InstallState::Unattempted {
            let format = fmt::format()
                .with_level(true)
                .with_line_number(true)
                .with_ansi(false)
                .without_time()
                .compact();

            let buf = self.buf.clone();
            let result = tracing_subscriber::fmt()
                .with_writer(move || -> LogWriter { LogWriter { buf: buf.clone() } })
                .event_format(format)
                .try_init()
                .map_err(|error| error.to_string());
            self.install = after_install(result);
            if let InstallState::Unavailable(message) = &self.install {
                self.buf
                    .lock()
                    .unwrap()
                    .push(format!("inspector logs unavailable: {message}"));
            }
        }

        self.flush_buffer(c).ok();
        Some(Duration::from_millis(100))
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("logs")
    }
}

#[derive_commands]
impl Logs {
    /// Construct a log panel.
    pub(crate) fn new() -> Self {
        Self {
            install: InstallState::Unattempted,
            buf: Arc::new(Mutex::new(vec![])),
        }
    }

    /// Execute a closure with the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut List<Text>, &mut dyn Context) -> Result<R>,
    {
        c.with_typed_slot::<ListSlot, _>(f)
    }

    /// Drain buffered log lines into the list.
    fn flush_buffer(&self, c: &mut dyn Context) -> Result<()> {
        let lines = mem::take(&mut *self.buf.lock().unwrap());
        if lines.is_empty() {
            return Ok(());
        }

        self.with_list(c, |list, ctx| {
            for line in lines {
                list.append(ctx, Text::new(line).with_wrap_width(78))?;
            }
            Ok(())
        })
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
    pub fn select_first(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| list.select_first(ctx))
    }

    #[command]
    /// Move selection to the last item.
    pub fn select_last(&self, c: &mut dyn Context) -> Result<()> {
        self.with_list(c, |list, ctx| list.select_last(ctx))
    }

    #[command]
    /// Move selection by a signed offset.
    pub fn select_by(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_list(c, |list, ctx| list.select_by(ctx, delta))
    }

    /// Scroll the view by one line in the specified direction.
    /// @param dir The direction to scroll.
    #[command]
    pub fn scroll(&self, c: &mut dyn Context, dir: FocusDirection) -> Result<()> {
        self.with_list(c, |list, ctx| {
            list.scroll(ctx, dir);
            Ok(())
        })
    }

    /// Page through the log view.
    /// Positive values move down; negative values move up.
    /// @param delta Signed page delta. Positive moves down and negative moves
    /// up.
    #[command]
    pub fn page(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        self.with_list(c, |list, ctx| list.page(ctx, delta))
    }
}

impl Loader for Logs {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Self>()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use canopy::{error::Result, testing::harness::Harness};

    use super::{InstallState, Logs, after_install};

    #[test]
    fn a_successful_install_takes_ownership() {
        assert_eq!(after_install(Ok(())), InstallState::Active);
    }

    #[test]
    fn a_failed_install_records_the_reason() {
        assert_eq!(
            after_install(Err("a global subscriber is already set".to_string())),
            InstallState::Unavailable("a global subscriber is already set".to_string())
        );
    }

    #[test]
    fn a_long_log_line_wraps_to_the_wrap_width() -> Result<()> {
        let mut harness = Harness::builder(Logs::new()).size(80, 4).build()?;
        harness.with_root_context(|logs: &mut Logs, ctx| {
            logs.buf.lock().unwrap().push("a".repeat(80));
            logs.flush_buffer(ctx)
        })?;
        harness.render()?;
        harness.render()?;

        let full = format!("█ {}", "a".repeat(78));
        harness
            .tbuf()
            .assert_matches(&[full.as_str(), "█ aa", "", ""]);
        Ok(())
    }
}
