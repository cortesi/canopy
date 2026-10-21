//! The toast stack that `Root` draws over the application.
//!
//! Each notice shows as a toast: a card at the right edge of the window, just
//! above the bottom row, for the notice's duration. A card has an edge in the
//! colour of its level, and its message wraps on at most three lines. With a
//! toast floor, which [`Root::set_toast_floor`](crate::Root::set_toast_floor)
//! sets, the stack rests just above the floor instead. The cards stack,
//! newest at the bottom, and each goes when its own time ends. The notice log
//! in the core decides which toasts are live, so this module only shows them.

use std::{mem, time::Duration};

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, Widget,
    error::Result,
    geom::{Line, Rect, Size},
    input::{Event, mouse},
    layout::{Align, Direction, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{NoticeId, NoticeLevel, TOAST_LIMIT},
    text,
};

/// Blank columns on each side of a toast's message.
const TOAST_PADDING: u32 = 1;

/// Columns of the coloured edge at a card's left.
const EDGE: u32 = 1;

/// The widest a card's message wraps to.
const MAX_TEXT_WIDTH: u32 = 48;

/// The most lines a card's message takes.
const MAX_LINES: usize = 3;

/// A full-size layer that stacks the live toasts at its bottom right.
///
/// The layer lets the pointer through, so only the boxes take clicks. It owns
/// the poll that ends each toast on time.
pub struct ToastLayer {
    /// The boxes, top first, one for each toast that can show.
    boxes: Vec<TypedId<ToastBox>>,
}

impl ToastLayer {
    /// Build the layer with its boxes, hidden until a toast shows, and return
    /// it detached.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let layer = context.create_detached(Self { boxes: Vec::new() })?;
        let mut boxes = Vec::with_capacity(TOAST_LIMIT);
        for _ in 0..TOAST_LIMIT {
            let toast = context.add_child(layer, ToastBox::default())?;
            context.set_hidden(toast.into(), true)?;
            boxes.push(toast);
        }
        context.with_widget_mut(layer, |layer: &mut Self, _| {
            layer.boxes = boxes;
            Ok(())
        })?;
        Ok(layer.into())
    }

    /// Show the live toasts in the layer at `node`, and schedule the poll that
    /// ends the next one.
    ///
    /// The notice hook calls this from `Root`'s context. The poll belongs to
    /// the layer, so the layer requests it from its own context.
    pub(crate) fn sync(context: &mut dyn Context, node: NodeId) -> Result<()> {
        context.with_widget_mut(node, |layer: &mut Self, context| {
            if layer.refresh(context)?.is_some() {
                context.request_poll()?;
            }
            Ok(())
        })
    }

    /// Show the live toasts, oldest at the top, and return the time until the
    /// next one ends.
    fn refresh(&self, context: &mut dyn Context) -> Result<Option<Duration>> {
        let now = context.now();
        let log = context.notice_log();
        let live = log
            .live_toasts(now)
            .into_iter()
            .map(|notice| Shown {
                id: notice.id,
                message: notice.message.clone(),
                count: notice.count_label(),
                level: notice.level,
            })
            .collect::<Vec<_>>();
        let next = log
            .next_toast_end(now)
            .map(|end| end.saturating_duration_since(now));
        for (index, toast) in self.boxes.iter().enumerate() {
            let shown = live.get(index).cloned();
            let hidden = shown.is_none();
            context.with_widget_mut(*toast, |toast: &mut ToastBox, _| {
                toast.shown = shown;
                Ok(())
            })?;
            context.set_hidden((*toast).into(), hidden)?;
        }
        Ok(next)
    }
}

impl Widget for ToastLayer {
    fn layout(&self) -> Layout {
        // A column stacks the boxes, and its end alignment puts the stack at
        // the bottom and each box at the right. The padding keeps the bottom
        // row and the right column clear.
        Layout::fill()
            .direction(Direction::Column)
            .align_horizontal(Align::End)
            .align_vertical(Align::End)
            .gap(1)
            .padding(Edges::new(0, 1, 1, 0))
    }

    fn accept_pointer(&self, _context: &dyn ViewContext) -> bool {
        false
    }

    fn poll(&mut self, context: &mut dyn Context) -> Result<Option<Duration>> {
        self.refresh(context)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("toasts")
    }
}

/// What one box shows.
#[derive(Clone, Debug)]
struct Shown {
    /// The notice the toast shows.
    id: NoticeId,
    /// The message.
    message: String,
    /// The count after the message, when the notice repeated.
    count: Option<String>,
    /// The level, which picks the style.
    level: NoticeLevel,
}

impl Shown {
    /// Return the columns the count takes after the message, with the blank
    /// column before it, or 0 without a count.
    fn count_width(&self) -> u32 {
        self.count
            .as_deref()
            .map_or(0, |count| text::width(count).saturating_add(1))
    }

    /// Return the lines of the message wrapped at `width` columns, with room
    /// for the count on the last line. A message too long for
    /// [`MAX_LINES`] lines ends with a marker.
    fn lines(&self, width: u32) -> Vec<String> {
        let width = width.max(1) as usize;
        let mut lines = wrap(&self.message, width);
        if lines.len() > MAX_LINES {
            let rest = lines.split_off(MAX_LINES - 1).join(" ");
            lines.push(text::truncate_end(&rest, width).into_owned());
        }
        if let Some(last) = lines.last_mut() {
            let room = width.saturating_sub(self.count_width() as usize);
            *last = text::truncate_end(last, room).into_owned();
        }
        lines
    }
}

/// One toast: a card as wide as its message, which wraps on at most
/// [`MAX_LINES`] lines.
#[derive(Default)]
pub struct ToastBox {
    /// The toast this box shows, while it shows one.
    shown: Option<Shown>,
}

impl ToastBox {
    /// Return the columns around the message: the edge and the padding.
    const fn frame() -> u32 {
        EDGE + 2 * TOAST_PADDING
    }
}

/// Wrap `message` at `width` columns, at word boundaries, keeping its line
/// breaks. A word wider than a line breaks across lines.
fn wrap(message: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in message.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word;
            while text::width(word) as usize > width {
                if !line.is_empty() {
                    lines.push(mem::take(&mut line));
                }
                let (head, _) = text::slice_by_columns(word, 0, width);
                // A grapheme wider than the line takes a line of its own.
                let head = if head.is_empty() {
                    word.chars().next().map_or("", |c| &word[..c.len_utf8()])
                } else {
                    head
                };
                lines.push(head.to_owned());
                word = &word[head.len()..];
            }
            if word.is_empty() {
                continue;
            }
            let needed = text::width(&line) as usize
                + usize::from(!line.is_empty())
                + text::width(word) as usize;
            if !line.is_empty() && needed > width {
                lines.push(mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl Widget for ToastBox {
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let Some(shown) = &self.shown else {
            return constraints.clamp(Size::new(0, 0));
        };
        let natural = text::width(&shown.message)
            .saturating_add(shown.count_width())
            .min(MAX_TEXT_WIDTH)
            .saturating_add(Self::frame());
        // The message wraps at the width the card gets, which a narrow
        // window can make less than the width it wants.
        let granted = constraints.clamp_size(Size::new(natural, 0)).w;
        let text_width = granted.saturating_sub(Self::frame());
        let lines = shown.lines(text_width);
        let widest = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let count = if index + 1 == lines.len() {
                    shown.count_width()
                } else {
                    0
                };
                text::width(line).saturating_add(count)
            })
            .max()
            .unwrap_or_default();
        let height = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        constraints.clamp(Size::new(widest.saturating_add(Self::frame()), height))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let Some(shown) = &self.shown else {
            return Ok(());
        };
        let (style, edge) = match shown.level {
            NoticeLevel::Info => ("root/toast/info", "root/toast/info/edge"),
            NoticeLevel::Warning => ("root/toast/warning", "root/toast/warning/edge"),
            NoticeLevel::Error => ("root/toast/error", "root/toast/error/edge"),
        };
        let rect = context.view().outer_rect_local();
        render.fill(style, rect, ' ')?;
        render.fill(
            edge,
            Rect::new(rect.tl.x, rect.tl.y, EDGE.min(rect.w), rect.h),
            ' ',
        )?;
        let text_width = rect.w.saturating_sub(Self::frame());
        let x = rect.tl.x.saturating_add(EDGE + TOAST_PADDING);
        let lines = shown.lines(text_width);
        let last = lines.len().saturating_sub(1);
        for (index, line) in lines.iter().enumerate().take(rect.h as usize) {
            let y = rect.tl.y + u32::try_from(index).unwrap_or(u32::MAX);
            render.text(style, Line::new(x, y, text_width), line)?;
            if index == last
                && let Some(count) = &shown.count
            {
                let x = x.saturating_add(text::width(line)).saturating_add(1);
                let width = shown.count_width().saturating_sub(1);
                render.text("root/toast/count", Line::new(x, y, width), count)?;
            }
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, context: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(mouse) = event else {
            return Ok(EventOutcome::Ignore);
        };
        if mouse.action != mouse::Action::Down {
            return Ok(EventOutcome::Handle);
        }
        if let Some(shown) = &self.shown {
            context.end_toast(shown.id);
        }
        Ok(EventOutcome::Handle)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("toast")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        error::Result,
        geom::{Point, Size},
        layout::LayoutOverride,
        runtime::{NoticePost, TOAST_LIMIT},
        testing::harness::Harness,
    };

    use crate::{
        Root,
        notice_test_support::{App, BandApp, advance, app, band_app, click, notify, rows},
    };

    #[test]
    fn toasts_stack_at_the_right_above_the_bottom_row() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 6))?;
        notify(&mut harness, NoticePost::info("first"))?;
        notify(&mut harness, NoticePost::warning("second"))?;
        let rows = rows(&harness);
        // The card is its edge, then the message with a blank column each
        // side, and it ends one column in from the right edge.
        assert_eq!(
            &rows[4][20..29],
            "  second ",
            "the newest sits lowest: {rows:?}"
        );
        assert!(rows[3].trim().is_empty(), "a blank row parts the cards");
        assert_eq!(&rows[2][21..29], "  first ", "the older one sits above it");
        assert!(rows[5].trim().is_empty(), "the bottom row stays clear");
        assert!(
            rows[4][..20].trim().is_empty(),
            "nothing else draws on the row"
        );

        let style = |path: &str| {
            harness
                .canopy
                .style()
                .resolve(path)
                .resolve_solid()
                .expect("a solid toast style")
        };
        let cell = |x| {
            harness
                .buf()
                .get(Point { x, y: 4 })
                .expect("a toast cell")
                .style
        };
        assert_eq!(cell(22).fg, style("root/toast/warning").fg);
        assert_eq!(cell(21).bg, style("root/toast/warning").bg);
        assert_eq!(
            cell(20).bg,
            style("root/toast/warning/edge").bg,
            "the edge takes the level's colour"
        );
        assert_ne!(
            style("root/toast/warning").fg,
            style("root/toast/info").fg,
            "a warning reads apart from an outcome"
        );
        Ok(())
    }

    #[test]
    fn a_toast_floor_keeps_the_stack_above_a_band_as_it_grows_and_hides() -> Result<()> {
        let (mut harness, band) = band_app(Size::new(30, 10))?;
        let row_of = |harness: &Harness, text: &str| {
            rows(harness)
                .iter()
                .position(|row| row.contains(text))
                .expect("the toast shows")
        };
        let band_top = |harness: &Harness| {
            rows(harness)
                .iter()
                .position(|row| row.starts_with('='))
                .expect("the band shows")
        };
        notify(&mut harness, NoticePost::info("first"))?;
        assert_eq!(
            row_of(&harness, "first"),
            8,
            "without a floor, above the bottom row"
        );

        harness.with_unique(|_: &mut BandApp, ctx| Root::set_toast_floor(ctx, Some(band)))?;
        harness.render()?;
        assert_eq!(band_top(&harness), 7);
        assert_eq!(row_of(&harness, "first"), 6, "the stack rests on the band");
        notify(&mut harness, NoticePost::info("second"))?;
        assert_eq!(row_of(&harness, "second"), 6, "the newest sits lowest");
        assert_eq!(row_of(&harness, "first"), 4);

        // The band grows, and the stack follows it at the next frame.
        harness.with_unique(|_: &mut BandApp, ctx| {
            ctx.set_layout_override(band, LayoutOverride::default().fixed_height(5))
        })?;
        harness.render()?;
        assert_eq!(band_top(&harness), 5);
        assert_eq!(row_of(&harness, "second"), 4, "{:?}", rows(&harness));

        // A hidden band sets no floor.
        harness.with_unique(|_: &mut BandApp, ctx| ctx.set_hidden(band, true))?;
        harness.render()?;
        assert_eq!(row_of(&harness, "second"), 8, "{:?}", rows(&harness));

        harness.with_unique(|_: &mut BandApp, ctx| ctx.set_hidden(band, false))?;
        harness.with_unique(|_: &mut BandApp, ctx| Root::set_toast_floor(ctx, None))?;
        harness.render()?;
        assert_eq!(
            row_of(&harness, "second"),
            8,
            "clearing the floor restores the place"
        );
        Ok(())
    }

    #[test]
    fn a_long_message_wraps_on_at_most_three_lines() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(80, 10))?;
        let words = "word ".repeat(40);
        notify(&mut harness, NoticePost::error(words.trim()))?;
        let screen = rows(&harness);
        let lines = screen
            .iter()
            .filter(|row| row.contains("word"))
            .collect::<Vec<_>>();
        assert_eq!(
            lines.len(),
            3,
            "the message wraps on three lines: {screen:?}"
        );
        assert!(
            lines[2].trim_end().ends_with('…'),
            "the cut shows: {screen:?}"
        );
        let widest = lines
            .iter()
            .map(|row| row.trim().chars().count())
            .max()
            .unwrap_or_default();
        assert!(widest <= 48, "the text wraps at 48 columns: {widest}");

        // A repeat keeps its count whole after the cut.
        notify(&mut harness, NoticePost::error(words.trim()))?;
        let screen = rows(&harness);
        assert!(
            screen.iter().any(|row| row.contains("… ×2")),
            "the count follows the cut: {screen:?}"
        );

        // A short message gives a narrow card on one line.
        let (mut harness, _clock) = app(Size::new(80, 6))?;
        notify(&mut harness, NoticePost::error("no vault here"))?;
        let screen = rows(&harness);
        assert_eq!(screen[4].trim(), "no vault here", "{screen:?}");
        // The card is the text and its frame of 3 columns, one column in from
        // the right, and its text starts after the edge and the padding.
        let card = 13 + 3;
        assert_eq!(
            screen[4].len() - screen[4].trim_start().len(),
            80 - 1 - card + 2
        );
        Ok(())
    }

    #[test]
    fn each_toast_ends_by_its_own_time() -> Result<()> {
        let (mut harness, clock) = app(Size::new(30, 6))?;
        notify(&mut harness, NoticePost::info("first"))?;
        advance(&mut harness, &clock, 1000)?;
        notify(&mut harness, NoticePost::info("second"))?;
        advance(&mut harness, &clock, 1000)?;
        let screen = rows(&harness).join("\n");
        assert!(!screen.contains("first"), "the first toast's time ended");
        assert!(screen.contains("second"), "the second has a second left");
        assert_eq!(
            &rows(&harness)[4][21..29],
            " second ",
            "the newest stays where it was"
        );
        advance(&mut harness, &clock, 1000)?;
        assert!(!rows(&harness).join("\n").contains("second"));
        assert_eq!(harness.canopy.notices().len(), 2, "the log keeps both");
        Ok(())
    }

    #[test]
    fn the_limit_ends_the_oldest_and_a_repeat_shows_its_count() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 8))?;
        for index in 0..=TOAST_LIMIT {
            notify(&mut harness, NoticePost::info(format!("toast {index}")))?;
        }
        let screen = rows(&harness).join("\n");
        assert!(!screen.contains("toast 0"), "the oldest ended early");
        assert!(screen.contains("toast 1") && screen.contains("toast 4"));

        notify(&mut harness, NoticePost::info("toast 4"))?;
        let rows = rows(&harness);
        assert!(
            rows[6].contains("toast 4 ×2"),
            "a repeat joins its toast at the bottom: {rows:?}"
        );
        assert!(!rows.join("\n").contains("toast 4\n"));

        // The count takes its own colour, apart from the message.
        let column = |text: &str| {
            let at = rows[6].find(text).expect("the text shows");
            u32::try_from(rows[6][..at].chars().count()).expect("a column")
        };
        let fg = |x| {
            harness
                .buf()
                .get(Point { x, y: 6 })
                .expect("a toast cell")
                .style
                .fg
        };
        let style = |path: &str| {
            harness
                .canopy
                .style()
                .resolve(path)
                .resolve_solid()
                .expect("a solid style")
                .fg
        };
        assert_eq!(fg(column("×2")), style("root/toast/count"));
        assert_eq!(fg(column("toast 4")), style("root/toast/info"));
        assert_ne!(style("root/toast/count"), style("root/toast/info"));
        Ok(())
    }

    #[test]
    fn clicks_pass_beside_the_toasts_and_a_click_on_one_ends_it() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 6))?;
        notify(&mut harness, NoticePost::info("first"))?;
        notify(&mut harness, NoticePost::info("second"))?;
        let clicks = |harness: &mut Harness| {
            harness
                .with_unique(|app: &mut App, _| Ok(app.clicks))
                .expect("the app is mounted")
        };

        click(&mut harness, 2, 4)?;
        assert_eq!(clicks(&mut harness), 1, "the layer lets the click through");
        click(&mut harness, 24, 4)?;
        harness.render()?;
        assert_eq!(clicks(&mut harness), 1, "a toast takes its own click");
        let screen = rows(&harness).join("\n");
        assert!(!screen.contains("second"), "the click ended the toast");
        assert!(screen.contains("first"), "and only that toast");
        assert_eq!(harness.canopy.notices().len(), 2, "the log keeps it");
        Ok(())
    }

    #[test]
    fn toasts_draw_over_an_application_modal_but_take_no_click() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 6))?;
        harness.script("app.cover()")?;
        notify(&mut harness, NoticePost::info("saved"))?;
        let rows = rows(&harness);
        assert_eq!(
            &rows[4][22..29],
            " saved ",
            "the toast draws over the cover"
        );
        assert!(rows[0].contains('#'), "the cover shows: {rows:?}");

        click(&mut harness, 24, 4)?;
        harness.render()?;
        let now = harness.canopy.notice_log().entries()[0].at;
        let log = harness.canopy.notice_log();
        assert!(
            log.is_live(&log.entries()[0], now),
            "the modal admits no click outside it, so the toast stays"
        );
        Ok(())
    }

    #[test]
    fn the_help_modal_draws_over_the_toasts() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 8))?;
        notify(&mut harness, NoticePost::info("saved"))?;
        harness.script("root.toggle_help()")?;
        harness.render()?;
        let order = |name: &str| -> Result<usize> {
            let node = harness.find_nodes(&format!("/root/{name}"))?[0];
            Ok(harness.canopy.with_root_view(|view| {
                view.children_of(view.root_id())
                    .iter()
                    .position(|child| *child == node)
                    .expect("an overlay is a child of the root")
            }))
        };
        assert!(
            order("toasts")? < order("help")?,
            "the help modal is above the toasts in the stack"
        );
        assert!(
            order("cover")? < order("toasts")?,
            "and the app's modal below"
        );
        Ok(())
    }

    #[test]
    fn toasts_render_in_a_tiny_view() -> Result<()> {
        for size in [Size::new(4, 3), Size::new(1, 1), Size::new(12, 2)] {
            let (mut harness, _clock) = app(size)?;
            notify(&mut harness, NoticePost::warning("a rather long refusal"))?;
        }
        Ok(())
    }
}
