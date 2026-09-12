//! Log panel for the inspector widget.

use std::{
    collections::VecDeque,
    io::{Result as IoResult, Write},
    mem,
    result::Result as StdResult,
    str,
    sync::{Arc, Mutex},
    time::Duration,
};

use canopy::{
    Canopy, Context, ContextExt, FocusDirection, Loader, NodeName, Render, ViewContext, Widget,
    derive_commands,
    error::{Error, Result},
    geom::Size,
    layout::{CanvasContext, Constraint, Layout, MeasureConstraints, Measurement},
};
use tracing_subscriber::fmt;

use crate::{List, Text};

canopy::slot!(ListSlot: List<Text, u64>);

/// Maximum number of queued or displayed log entries.
const MAX_LOG_ENTRIES: usize = 1_000;
/// Maximum retained UTF-8 bytes per log entry.
const MAX_LOG_ENTRY_BYTES: usize = 4_096;
/// Visible suffix for entries that exceed the byte limit.
const TRUNCATED: &str = " …";

/// Append a bounded formatted record, dropping the oldest queued entry when
/// full.
fn enqueue_record(buffer: &Mutex<VecDeque<String>>, bytes: &[u8]) {
    let mut prefix = &bytes[..bytes.len().min(MAX_LOG_ENTRY_BYTES)];
    if bytes.len() > prefix.len()
        && let Err(error) = str::from_utf8(prefix)
        && error.error_len().is_none()
    {
        prefix = &prefix[..error.valid_up_to()];
    }
    let mut line = String::from_utf8_lossy(prefix).trim().to_owned();
    if bytes.len() > prefix.len() || line.len() > MAX_LOG_ENTRY_BYTES {
        let mut end = line.len().min(MAX_LOG_ENTRY_BYTES - TRUNCATED.len());
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
        line.push_str(TRUNCATED);
        line.shrink_to_fit();
    }
    let mut queue = buffer.lock().unwrap();
    if queue.len() == MAX_LOG_ENTRIES {
        queue.pop_front();
    }
    queue.push_back(line);
}

/// Log writer that appends to a shared buffer.
struct LogWriter {
    /// Shared log buffer.
    buf: Arc<Mutex<VecDeque<String>>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        enqueue_record(&self.buf, buf);
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
    buf: Arc<Mutex<VecDeque<String>>>,
    /// Next stable identity for a displayed log entry.
    next_key: u64,
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
        c.add_slot::<ListSlot>(List::<Text, u64>::new().with_selection_indicator(
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
                enqueue_record(
                    &self.buf,
                    format!("inspector logs unavailable: {message}").as_bytes(),
                );
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
            buf: Arc::new(Mutex::new(VecDeque::new())),
            next_key: 0,
        }
    }

    /// Execute a closure with the list widget.
    fn with_list<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut List<Text, u64>, &mut dyn Context) -> Result<R>,
    {
        c.with_typed_slot::<ListSlot, _>(f)
    }

    /// Drain buffered log lines into the list.
    fn flush_buffer(&mut self, c: &mut dyn Context) -> Result<()> {
        let mut lines = mem::take(&mut *self.buf.lock().unwrap());
        if lines.is_empty() {
            return Ok(());
        }

        let start = self.next_key;
        let result = (|| {
            let end = start
                .checked_add(lines.len() as u64)
                .ok_or_else(|| Error::Internal("inspector log keys exhausted".into()))?;
            self.with_list(c, |list, ctx| {
                let excess = list
                    .len()
                    .saturating_add(lines.len())
                    .saturating_sub(MAX_LOG_ENTRIES);
                let desired: Vec<_> = list
                    .keys()
                    .iter()
                    .copied()
                    .chain(start..end)
                    .skip(excess)
                    .collect();
                list.reconcile(
                    ctx,
                    desired,
                    |key| {
                        let index = key
                            .checked_sub(start)
                            .and_then(|index| usize::try_from(index).ok());
                        let line = index.and_then(|index| lines.get(index)).ok_or_else(|| {
                            Error::Internal("pending inspector log entry is missing".into())
                        })?;
                        Ok(Text::new(line.clone()).with_wrap_width(78))
                    },
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            })?;
            self.next_key = end;
            Ok(())
        })();
        if result.is_err() {
            // Keep the newest records if writes arrived while reconciliation
            // failed.
            let mut pending = self.buf.lock().unwrap();
            lines.append(&mut pending);
            let excess = lines.len().saturating_sub(MAX_LOG_ENTRIES);
            lines.drain(..excess);
            *pending = lines;
        }
        result
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
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };

    use canopy::{ContextExt, error::Result, testing::harness::Harness};

    use super::{InstallState, ListSlot, LogWriter, Logs, after_install};
    use crate::List;

    #[test]
    fn queued_logs_retain_only_the_latest_entries() {
        let buf = Arc::new(Mutex::new(Default::default()));
        let mut writer = LogWriter { buf: buf.clone() };
        for index in 0..1_005 {
            writer
                .write_all(format!("entry {index}\n").as_bytes())
                .unwrap();
        }
        let lines = buf.lock().unwrap();
        assert_eq!(lines.len(), 1_000);
        assert_eq!(lines.front().unwrap(), "entry 5");
        assert_eq!(lines.back().unwrap(), "entry 1004");
    }

    #[test]
    fn queued_logs_bound_large_unicode_and_invalid_utf8_entries() {
        for bytes in ["界".repeat(10_000).into_bytes(), vec![0xff; 10_000]] {
            let buf = Arc::new(Mutex::new(Default::default()));
            let mut writer = LogWriter { buf: buf.clone() };
            assert_eq!(writer.write(&bytes).unwrap(), bytes.len());
            let lines = buf.lock().unwrap();
            let line = lines.front().unwrap();
            assert!(line.len() <= 4_096);
            assert!(line.ends_with(" …"));
        }
    }

    #[test]
    fn truncation_does_not_turn_a_partial_character_into_a_replacement() {
        let buf = Arc::new(Mutex::new(Default::default()));
        let mut writer = LogWriter { buf: buf.clone() };
        let line = format!("{}界more", " ".repeat(4_095));
        writer.write_all(line.as_bytes()).unwrap();
        assert_eq!(buf.lock().unwrap().front().unwrap(), " …");
    }

    #[test]
    fn displayed_logs_evict_old_rows_and_preserve_surviving_selection() -> Result<()> {
        let logs = Logs {
            install: InstallState::Active,
            ..Logs::new()
        };
        let mut harness = Harness::builder(logs).size(80, 4).build()?;
        harness.with_root_context(|logs: &mut Logs, ctx| {
            let mut writer = LogWriter {
                buf: logs.buf.clone(),
            };
            for index in 0..1_005 {
                writer
                    .write_all(format!("entry {index}").as_bytes())
                    .unwrap();
            }
            logs.flush_buffer(ctx)?;
            let selected = logs.with_list(ctx, |list, ctx| {
                assert_eq!(list.len(), 1_000);
                list.select_key(ctx, &500)?;
                Ok(list.selected_item().unwrap())
            })?;
            for index in 1_005..1_015 {
                writer
                    .write_all(format!("entry {index}").as_bytes())
                    .unwrap();
            }
            logs.flush_buffer(ctx)?;
            logs.with_list(ctx, |list, _| {
                assert_eq!(list.len(), 1_000);
                assert_eq!(list.keys().first(), Some(&10));
                assert_eq!(list.keys().last(), Some(&1_009));
                assert_eq!(list.selected_key(), Some(&500));
                assert_eq!(list.selected_item(), Some(selected));
                Ok(())
            })
        })?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("entry 15"));
        harness.with_root_context(|logs: &mut Logs, ctx| logs.select_last(ctx))?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("entry 1014"));
        Ok(())
    }

    #[test]
    fn failed_log_updates_keep_pending_entries_for_retry() -> Result<()> {
        let mut harness = Harness::builder(Logs::new()).size(80, 4).build()?;
        harness.with_root_context(|logs: &mut Logs, ctx| {
            let list = ctx.get_slot::<ListSlot>()?.expect("list mounted");
            ctx.remove_subtree(list.into())?;
            LogWriter {
                buf: logs.buf.clone(),
            }
            .write_all(b"keep this entry")
            .unwrap();
            assert!(logs.flush_buffer(ctx).is_err());
            assert_eq!(logs.next_key, 0);
            assert_eq!(
                logs.buf.lock().unwrap().front().map(String::as_str),
                Some("keep this entry")
            );
            ctx.add_slot::<ListSlot>(List::new())?;
            logs.flush_buffer(ctx)?;
            assert_eq!(logs.next_key, 1);
            assert!(logs.buf.lock().unwrap().is_empty());
            logs.with_list(ctx, |list, _| {
                assert_eq!(list.keys(), [0]);
                Ok(())
            })
        })
    }

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
            logs.buf.lock().unwrap().push_back("a".repeat(80));
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
