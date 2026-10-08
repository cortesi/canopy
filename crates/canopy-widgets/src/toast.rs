//! The toast stack that `Root` draws over the application.
//!
//! An info or warning notice shows as a toast: a one-row box at the right edge
//! of the window, just above the bottom row, for the notice's duration. The
//! toasts stack, newest at the bottom, and each goes when its own time ends.
//! The notice log in the core decides which toasts are live, so this module
//! only shows them.

use std::time::Duration;

use canopy::{
    Context, ContextExt, EventOutcome, NodeId, NodeName, TypedId, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    input::{Event, mouse},
    layout::{Align, Direction, Edges, Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::{NoticeId, NoticeLevel, TOAST_LIMIT},
    text::{self, Truncate},
};

/// Blank columns on each side of a toast's message.
const TOAST_PADDING: u32 = 1;

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
                text: notice.text(),
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
    /// The message, with its count when it repeated.
    text: String,
    /// The level, which picks the style.
    level: NoticeLevel,
}

/// One toast: a one-row box as wide as its message.
#[derive(Default)]
pub struct ToastBox {
    /// The toast this box shows, while it shows one.
    shown: Option<Shown>,
}

impl Widget for ToastBox {
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let width = self
            .shown
            .as_ref()
            .map_or(0, |shown| text::width(&shown.text))
            .saturating_add(2 * TOAST_PADDING);
        constraints.clamp(Size::new(width, 1))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let Some(shown) = &self.shown else {
            return Ok(());
        };
        let style = match shown.level {
            NoticeLevel::Warning => "root/toast/warning",
            NoticeLevel::Info | NoticeLevel::Error => "root/toast/info",
        };
        let rect = context.view().outer_rect_local();
        render.fill(style, rect, ' ')?;
        let budget = rect.w.saturating_sub(2 * TOAST_PADDING) as usize;
        let message = Truncate::End.apply(&shown.text, budget);
        render.text(
            style,
            Line::new(
                rect.tl.x.saturating_add(TOAST_PADDING),
                rect.tl.y,
                rect.w.saturating_sub(TOAST_PADDING),
            ),
            &message,
        )
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
        runtime::{NoticePost, TOAST_LIMIT},
        testing::harness::Harness,
    };

    use crate::notice_test_support::{App, advance, app, click, notify, rows};

    #[test]
    fn toasts_stack_at_the_right_above_the_bottom_row() -> Result<()> {
        let (mut harness, _clock) = app(Size::new(30, 6))?;
        notify(&mut harness, NoticePost::info("first"))?;
        notify(&mut harness, NoticePost::warning("second"))?;
        let rows = rows(&harness);
        // The box is the message and a blank column each side, and it ends one
        // column in from the right edge.
        assert_eq!(
            &rows[4][21..29],
            " second ",
            "the newest sits lowest: {rows:?}"
        );
        assert_eq!(&rows[3][22..29], " first ", "the older one sits above it");
        assert!(rows[5].trim().is_empty(), "the bottom row stays clear");
        assert!(
            rows[4][..21].trim().is_empty(),
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
        assert_ne!(
            style("root/toast/warning").fg,
            style("root/toast/info").fg,
            "a warning reads apart from an outcome"
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
