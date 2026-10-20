//! A line of status items gathered from source widgets.

use canopy::{
    NodeId, NodeName, ViewContext, Widget,
    error::Result,
    geom::Line,
    layout::Layout,
    render::Render,
    status::{StatusItem, StatusSide, StatusSpan, spans_width},
};

/// A line of the status items its source widgets expose.
///
/// The line asks each source for its [`Widget::status`] items as it renders,
/// so it follows their state with no update calls. Start items run from the
/// left edge and end items end at the right edge, each group in the order the
/// sources give, with the separator between neighbours. When the items do not
/// fit, the line shortens the item of lowest priority that has a compact form,
/// and otherwise hides it, until the rest fit.
///
/// Each run resolves its own style path under the layer that holds the line,
/// so inside a [`StatusBar`](crate::StatusBar) a run named `value` resolves
/// `status_bar/value`.
pub struct StatusLine {
    /// Widgets whose items the line shows, in order.
    sources: Vec<NodeId>,
    /// Runs drawn between two neighbouring items on one side.
    separator: Vec<StatusSpan>,
    /// Columns kept clear at the right end while end items show.
    margin: u32,
}

/// How one item shows in a fitted line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// The item's full runs.
    Full,
    /// The item's compact runs.
    Compact,
    /// Nothing.
    Hidden,
}

impl StatusLine {
    /// Construct a line with no sources, whose items stand two columns apart.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            separator: vec![StatusSpan::plain("  ")],
            margin: 0,
        }
    }

    /// Add a source widget after the sources already added.
    #[must_use]
    pub fn with_source(mut self, node: impl Into<NodeId>) -> Self {
        self.sources.push(node.into());
        self
    }

    /// Draw `separator` between two neighbouring items on one side.
    #[must_use]
    pub fn with_separator(mut self, separator: impl IntoIterator<Item = StatusSpan>) -> Self {
        self.separator = separator.into_iter().collect();
        self
    }

    /// Keep `margin` columns clear at the right end while end items show, so
    /// they stand apart from what follows the line, such as a key hint.
    #[must_use]
    pub fn with_margin(mut self, margin: u32) -> Self {
        self.margin = margin;
        self
    }

    /// Return the items the sources expose, in order, leaving out empty ones.
    pub fn items(&self, ctx: &dyn ViewContext) -> Result<Vec<StatusItem>> {
        let mut items = Vec::new();
        for source in &self.sources {
            ctx.with_widget_dyn(*source, &mut |widget| {
                items.extend(widget.status(ctx));
                Ok(())
            })?;
        }
        items.retain(|item| !item.is_empty());
        Ok(items)
    }

    /// Return the runs `item` shows in `form`.
    fn runs(item: &StatusItem, form: Form) -> &[StatusSpan] {
        match (form, &item.compact) {
            (Form::Hidden, _) => &[],
            (Form::Compact, Some(compact)) => compact,
            _ => &item.spans,
        }
    }

    /// Return the items of `side` that show, with how each shows.
    fn shown<'a>(
        items: &'a [StatusItem],
        forms: &[Form],
        side: StatusSide,
    ) -> Vec<(&'a StatusItem, Form)> {
        items
            .iter()
            .zip(forms.iter().copied())
            .filter(|(item, form)| item.side == side && *form != Form::Hidden)
            .collect()
    }

    /// Return the columns one side's shown items take, separators included.
    fn side_width(&self, shown: &[(&StatusItem, Form)]) -> u32 {
        let count = u32::try_from(shown.len()).unwrap_or(u32::MAX);
        shown
            .iter()
            .map(|(item, form)| spans_width(Self::runs(item, *form)))
            .fold(0, u32::saturating_add)
            .saturating_add(spans_width(&self.separator).saturating_mul(count.saturating_sub(1)))
    }

    /// Return the columns the shown items take, separators and the column
    /// between the two sides included.
    fn width(&self, items: &[StatusItem], forms: &[Form]) -> u32 {
        let start = self.side_width(&Self::shown(items, forms, StatusSide::Start));
        let end = self.side_width(&Self::shown(items, forms, StatusSide::End));
        let gap = u32::from(start > 0 && end > 0);
        let margin = if end > 0 { self.margin } else { 0 };
        start
            .saturating_add(end)
            .saturating_add(gap)
            .saturating_add(margin)
    }

    /// Choose how each item shows within `width` columns.
    fn fit(&self, items: &[StatusItem], width: u32) -> Vec<Form> {
        let mut forms = vec![Form::Full; items.len()];
        while self.width(items, &forms) > width {
            // The lowest priority gives way first, and of equals, the last.
            let Some(index) = (0..items.len())
                .filter(|index| forms[*index] != Form::Hidden)
                .min_by_key(|index| (items[*index].priority, usize::MAX - index))
            else {
                break;
            };
            forms[index] = if forms[index] == Form::Full && items[index].compact.is_some() {
                Form::Compact
            } else {
                Form::Hidden
            };
        }
        forms
    }

    /// Draw `runs` from `x`, cut at `end`, on row `y`, and return where they
    /// end.
    fn draw(render: &mut Render, runs: &[StatusSpan], mut x: u32, end: u32, y: u32) -> Result<u32> {
        for run in runs {
            if x >= end {
                break;
            }
            let width = run.width().min(end - x);
            if width > 0 {
                render.text(&run.style, Line::new(x, y, width), &run.text)?;
            }
            x = x.saturating_add(width);
        }
        Ok(x)
    }

    /// Draw one side's shown items from `x`, cut at `end`.
    fn draw_side(
        &self,
        render: &mut Render,
        shown: &[(&StatusItem, Form)],
        mut x: u32,
        end: u32,
        y: u32,
    ) -> Result<()> {
        for (index, (item, form)) in shown.iter().enumerate() {
            if index > 0 {
                x = Self::draw(render, &self.separator, x, end, y)?;
            }
            x = Self::draw(render, Self::runs(item, *form), x, end, y)?;
        }
        Ok(())
    }
}

impl Default for StatusLine {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for StatusLine {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        let items = self.items(ctx)?;
        let forms = self.fit(&items, area.w);
        let start = Self::shown(&items, &forms, StatusSide::Start);
        let end = Self::shown(&items, &forms, StatusSide::End);
        let margin = if end.is_empty() { 0 } else { self.margin };
        let right = area.tl.x.saturating_add(area.w).saturating_sub(margin);
        // A lone item wider than the line keeps its start and loses its end.
        let end_x = right.saturating_sub(self.side_width(&end)).max(area.tl.x);
        self.draw_side(render, &start, area.tl.x, end_x, area.tl.y)?;
        self.draw_side(render, &end, end_x, right, area.tl.y)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("status_line")
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use canopy::{Context, ContextExt, geom::Size, testing::harness::Harness};

    use super::*;

    /// A root that exposes a counter as status and mounts a line that reads
    /// it.
    struct Source {
        /// The count the root reports, shared with the test.
        count: Rc<Cell<u32>>,
    }

    impl Widget for Source {
        fn layout(&self) -> Layout {
            Layout::column()
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let root = ctx.node_id();
            ctx.add_child(root, StatusLine::new().with_source(root).with_margin(1))?;
            Ok(())
        }

        fn status(&self, _ctx: &dyn ViewContext) -> Vec<StatusItem> {
            vec![
                StatusItem::new("name").priority(2).span("text", "project"),
                StatusItem::new("count")
                    .at_end()
                    .priority(1)
                    .span("text", format!("{} items", self.count.get()))
                    .compact([StatusSpan::plain(self.count.get().to_string())]),
            ]
        }
    }

    fn item(id: &'static str, text: &str, priority: i32) -> StatusItem {
        StatusItem::new(id).span("text", text).priority(priority)
    }

    #[test]
    fn a_short_line_compacts_then_hides_the_lowest_priority_first() {
        let line = StatusLine::new();
        let items = vec![
            item("path", "~/project", 10),
            item("counts", "12 lines · 40 words", 5)
                .at_end()
                .compact([StatusSpan::plain("12L 40W")]),
            item("age", "modified now", 1).at_end(),
        ];
        assert_eq!(line.fit(&items, 80), [Form::Full; 3]);
        // 9 + 1 + 19 + 2 + 12 = 43 columns in full.
        assert_eq!(line.fit(&items, 43), [Form::Full; 3]);
        assert_eq!(
            line.fit(&items, 42),
            [Form::Full, Form::Full, Form::Hidden],
            "the age has no compact form, so it hides"
        );
        assert_eq!(
            line.fit(&items, 20),
            [Form::Full, Form::Compact, Form::Hidden],
            "the counts shorten before they hide"
        );
        assert_eq!(
            line.fit(&items, 9),
            [Form::Full, Form::Hidden, Form::Hidden]
        );
    }

    #[test]
    fn the_line_follows_its_source_and_compacts_to_fit() -> Result<()> {
        let count = Rc::new(Cell::new(3));
        let mut harness = Harness::builder(Source {
            count: count.clone(),
        })
        .size(24, 1)
        .build()?;
        harness.render()?;
        assert_eq!(
            harness.tbuf().lines()[0],
            "project         3 items ",
            "start items lead and end items end before the margin"
        );
        count.set(12);
        harness.render()?;
        assert!(
            harness.tbuf().lines()[0].contains("12 items"),
            "the line reads its source as it renders"
        );
        harness.canopy.set_screen_size(Size::new(12, 1))?;
        harness.render()?;
        assert_eq!(
            harness.tbuf().lines()[0],
            "project  12 ",
            "the count compacts"
        );
        Ok(())
    }
}
