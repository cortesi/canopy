//! The notice log panel: every notice the log keeps, newest first.
//!
//! `Root` owns the panel and opens it as a modal, as it does the help modal.
//! The panel holds a copy of the log's rows, because a widget measures
//! without a context to read the log. Root's notice hook refreshes the copy
//! whenever the log changes.

use std::{
    mem,
    time::{Duration, Instant},
};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, ViewContextExt,
    Widget,
    error::{Error, Result},
    geom::{Line, Size},
    input::Event,
    layout::{
        CanvasContext, Constraint, Edges, Layout, MeasureConstraints, MeasureOverflow, Measurement,
    },
    render::Render,
    runtime::{Notice, NoticeLevel},
    text::{self, Truncate},
};

use crate::Dialog;

/// Columns the age takes, right-aligned, such as "12s" or "59m".
const AGE_WIDTH: usize = 4;
/// Blank columns between the age and the message.
const AGE_GAP: usize = 2;
/// What an empty log shows.
const PLACEHOLDER: &str = "no notices";

/// Transparent overlay that owns the notice log panel.
///
/// The overlay draws nothing of its own, so the dimmed application stays
/// visible around the panel. It still consumes mouse input.
pub struct NoticeLog;

impl NoticeLog {
    /// Build the panel subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let overlay = context.create_detached(Self)?;
        Dialog::new()
            .with_title("Notices")
            .with_max_width(72)
            .with_margin(Edges::symmetric(2, 1))
            .add(context, overlay.into(), NoticeList::default())?;
        Ok(overlay.into())
    }

    /// Return the list within an installed panel.
    pub(crate) fn list_id(context: &dyn Context, panel: NodeId) -> Result<TypedId<NoticeList>> {
        context
            .unique_descendant::<NoticeList>(panel)?
            .ok_or_else(|| Error::NotFound("notice log list".into()))
    }

    /// Copy the log into the panel at `panel`, newest first.
    pub(crate) fn sync(context: &mut dyn Context, panel: NodeId) -> Result<()> {
        let list = Self::list_id(context, panel)?;
        context.with_widget_mut(list, |list: &mut NoticeList, context| {
            list.rows = rows(context);
            Ok(())
        })
    }
}

impl Widget for NoticeLog {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("notice_log");
        Ok(())
    }

    fn on_event(&mut self, event: &Event, _context: &mut dyn Context) -> Result<EventOutcome> {
        if matches!(event, Event::Mouse(_)) {
            Ok(EventOutcome::Handle)
        } else {
            Ok(EventOutcome::Ignore)
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("notice_log")
    }
}

/// One row of the panel: a copy of one notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogRow {
    /// When the notice last posted.
    at: Instant,
    /// The message.
    message: String,
    /// The count after the message, when the notice repeated.
    count: Option<String>,
    /// The level, which picks the style.
    level: NoticeLevel,
}

impl LogRow {
    /// Return the columns the count takes after the message, with the blank
    /// column before it, or 0 without a count.
    fn count_width(&self) -> usize {
        self.count
            .as_deref()
            .map_or(0, |count| text::width(count) as usize + 1)
    }
}

impl From<&Notice> for LogRow {
    fn from(notice: &Notice) -> Self {
        Self {
            at: notice.at,
            message: notice.message.clone(),
            count: notice.count_label(),
            level: notice.level,
        }
    }
}

/// Return the log's rows, newest first.
pub fn rows(context: &dyn ViewContext) -> Vec<LogRow> {
    context
        .notice_log()
        .entries()
        .iter()
        .rev()
        .map(LogRow::from)
        .collect()
}

/// Return how long ago `at` was, as the panel shows it.
fn age(at: Instant, now: Instant) -> String {
    let seconds = now.saturating_duration_since(at).as_secs();
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..86_400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// Return how long until the age of `at` next changes its label.
fn next_label(at: Instant, now: Instant) -> Duration {
    let elapsed = now.saturating_duration_since(at);
    let unit = match elapsed.as_secs() {
        0..60 => 1,
        60..3600 => 60,
        3600..86_400 => 3600,
        _ => 86_400,
    };
    let unit = Duration::from_secs(unit);
    let into =
        Duration::from_nanos(u64::try_from(elapsed.as_nanos() % unit.as_nanos()).unwrap_or(0));
    unit - into
}

/// The scrolling list inside the panel.
#[derive(Default)]
pub struct NoticeList {
    /// A copy of the log, newest first.
    rows: Vec<LogRow>,
}

impl NoticeList {
    /// Replace the rows, and return the rows they replace.
    pub(crate) fn replace_rows(&mut self, rows: Vec<LogRow>) -> Vec<LogRow> {
        mem::replace(&mut self.rows, rows)
    }

    /// Return the width that shows every row whole.
    fn natural_width(&self) -> u32 {
        let widest = self
            .rows
            .iter()
            .map(|row| text::width(&row.message) as usize + row.count_width())
            .max()
            .map_or(text::width(PLACEHOLDER) as usize, |widest| {
                AGE_WIDTH + AGE_GAP + widest
            });
        u32::try_from(widest).unwrap_or(u32::MAX)
    }

    /// Return the row count, which is one per notice or one placeholder row.
    fn row_count(&self) -> u32 {
        u32::try_from(self.rows.len().max(1)).unwrap_or(u32::MAX)
    }
}

impl Widget for NoticeList {
    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn layout(&self) -> Layout {
        Layout::fill()
            .overflow_y(MeasureOverflow::Unbounded)
            .padding(Edges::symmetric(0, 1))
    }

    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let natural = self.natural_width();
        let width = match constraints.width {
            Constraint::Exact(width) => width,
            Constraint::AtMost(width) => natural.min(width),
            Constraint::Unbounded => natural,
        };
        constraints.clamp(Size::new(width, self.row_count()))
    }

    fn canvas(&self, view: Size, _context: &CanvasContext) -> Size {
        Size::new(view.w, self.row_count())
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.push_layer("notice_log");
        let view = context.view();
        render.fill("panel", view.outer_rect_local(), ' ')?;
        let content = view.view_rect_local();
        if self.rows.is_empty() {
            return render.text(
                "placeholder",
                Line::new(content.tl.x, content.tl.y, content.w),
                PLACEHOLDER,
            );
        }
        let now = context.now();
        let viewport = view.view_rect();
        let width = content.w as usize;
        for (offset, row) in self
            .rows
            .iter()
            .skip(viewport.tl.y as usize)
            .take(viewport.h as usize)
            .enumerate()
        {
            let y = content.tl.y + u32::try_from(offset).unwrap_or(u32::MAX);
            let age = format!("{:>AGE_WIDTH$}", age(row.at, now));
            render.text("age", Line::new(content.tl.x, y, content.w), &age)?;
            let lead = AGE_WIDTH + AGE_GAP;
            let budget = width.saturating_sub(lead);
            let style = match row.level {
                NoticeLevel::Info => "info",
                NoticeLevel::Warning => "warning",
                NoticeLevel::Error => "error",
            };
            // A narrow panel cuts the message and keeps the count whole.
            let count_width = row.count_width().min(budget);
            let message = Truncate::End.apply(&row.message, budget - count_width);
            let columns = |columns: usize| u32::try_from(columns).unwrap_or(0);
            let x = content.tl.x + u32::try_from(lead).unwrap_or(u32::MAX);
            let line = Line::new(x, y, columns(budget - count_width));
            render.text(style, line, &message)?;
            if let Some(count) = &row.count {
                let x = x + columns(text::width(&message) as usize + 1);
                let line = Line::new(x, y, columns(count_width.saturating_sub(1)));
                render.text("count", line, count)?;
            }
        }
        Ok(())
    }

    fn poll(&mut self, context: &mut dyn Context) -> Result<Option<Duration>> {
        // The ages advance only while the panel shows. A closed panel's view
        // is empty, and the next open asks for a poll again.
        if context.view().outer.w == 0 || self.rows.is_empty() {
            return Ok(None);
        }
        let now = context.now();
        Ok(self.rows.iter().map(|row| next_label(row.at, now)).min())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("notice_list")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::Point, input::key::KeyCode, runtime::NoticePost, testing::harness::Harness,
    };

    use super::*;
    use crate::notice_test_support::{advance, app, notify, rows};

    /// Return the row that holds `text`, and its index.
    fn row_of(harness: &Harness, text: &str) -> (usize, String) {
        rows(harness)
            .into_iter()
            .enumerate()
            .find(|(_, row)| row.contains(text))
            .unwrap_or_else(|| panic!("{text:?} shows in {:#?}", rows(harness)))
    }

    /// Return whether the panel shows.
    fn open(harness: &Harness) -> bool {
        rows(harness).iter().any(|row| row.contains("Notices"))
    }

    #[test]
    fn the_panel_lists_the_log_newest_first_by_age_and_level() -> Result<()> {
        let (mut harness, clock) = app(Size::new(40, 12))?;
        notify(&mut harness, NoticePost::info("saved"))?;
        advance(&mut harness, &clock, 3000)?;
        notify(&mut harness, NoticePost::warning("top of repo"))?;
        notify(&mut harness, NoticePost::warning("top of repo"))?;
        harness.script("root.toggle_notices()")?;
        harness.render()?;
        assert!(open(&harness), "the panel shows: {:#?}", rows(&harness));
        let (newest, row) = row_of(&harness, "top of repo ×2");
        assert!(row.contains("0s"), "{row:?}");
        let (oldest, row) = row_of(&harness, "saved");
        assert!(row.contains("3s"), "{row:?}");
        assert!(newest < oldest, "the newest notice leads");

        let x = row_of(&harness, "top of repo")
            .1
            .find("top")
            .expect("the message");
        let cell = harness
            .buf()
            .get(Point {
                x: u32::try_from(x).expect("a column"),
                y: u32::try_from(newest).expect("a row"),
            })
            .expect("a message cell");
        let warning = harness
            .canopy
            .style()
            .resolve("notice_log/warning")
            .resolve_solid()
            .expect("a solid style");
        assert_eq!(cell.style.fg, warning.fg, "the level colors the message");

        let row = &rows(&harness)[newest];
        let at = row.find("×2").expect("the count shows");
        let count_cell = harness
            .buf()
            .get(Point {
                x: u32::try_from(row[..at].chars().count()).expect("a column"),
                y: u32::try_from(newest).expect("a row"),
            })
            .expect("a count cell");
        let count = harness
            .canopy
            .style()
            .resolve("notice_log/count")
            .resolve_solid()
            .expect("a solid style");
        assert_eq!(
            count_cell.style.fg, count.fg,
            "the count takes its own colour"
        );
        assert_ne!(count.fg, warning.fg);
        Ok(())
    }

    #[test]
    fn ages_advance_while_the_panel_shows() -> Result<()> {
        let (mut harness, clock) = app(Size::new(40, 12))?;
        notify(&mut harness, NoticePost::info("saved"))?;
        harness.script("root.toggle_notices()")?;
        harness.render()?;
        assert!(row_of(&harness, "saved").1.contains(" 0s"));
        advance(&mut harness, &clock, 61_000)?;
        assert!(row_of(&harness, "saved").1.contains(" 1m"));
        Ok(())
    }

    #[test]
    fn a_notice_that_arrives_while_open_leads_and_the_scroll_keys_reach_the_end() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(40, 12))?;
        for index in 0..20 {
            notify(&mut harness, NoticePost::info(format!("notice {index}")))?;
        }
        harness.script("root.toggle_notices()")?;
        harness.render()?;
        assert!(rows(&harness).join("\n").contains("notice 19"));
        assert!(!rows(&harness).join("\n").contains("notice 0\n"));

        notify(&mut harness, NoticePost::error("disk full"))?;
        let (top, _) = row_of(&harness, "disk full");
        let (next, _) = row_of(&harness, "notice 19");
        assert!(top < next, "the new notice leads");

        harness.key(KeyCode::End)?;
        harness.render()?;
        assert!(
            row_of(&harness, "notice 0").0 > 0,
            "End reaches the oldest notice, which the new one pushed down"
        );
        harness.key(KeyCode::Esc)?;
        harness.render()?;
        assert!(!open(&harness), "Esc closes the panel");
        Ok(())
    }

    #[test]
    fn the_panel_opens_over_an_application_modal_and_closes_with_it() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(40, 12))?;
        harness.script("app.cover()")?;
        harness.script("root.show_notices()")?;
        harness.render()?;
        assert!(open(&harness), "the panel opens over the app's modal");
        assert!(rows(&harness).join("\n").contains("no notices"));
        harness.script("app.uncover()")?;
        harness.render()?;
        assert!(!open(&harness), "closing the owner closes the panel");
        Ok(())
    }

    #[test]
    fn ages_read_in_their_largest_whole_unit() {
        let at = Instant::now();
        let after = |seconds| at + Duration::from_secs(seconds);
        assert_eq!(age(at, after(0)), "0s");
        assert_eq!(age(at, after(59)), "59s");
        assert_eq!(age(at, after(60)), "1m");
        assert_eq!(age(at, after(3599)), "59m");
        assert_eq!(age(at, after(3600)), "1h");
        assert_eq!(age(at, after(90_000)), "1d");
    }

    #[test]
    fn the_next_label_falls_on_the_unit_boundary() {
        let at = Instant::now();
        let after = |millis| at + Duration::from_millis(millis);
        assert_eq!(next_label(at, after(250)), Duration::from_millis(750));
        assert_eq!(next_label(at, after(61_000)), Duration::from_secs(59));
    }
}
