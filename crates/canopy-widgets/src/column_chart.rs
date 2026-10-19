//! Stacked columns above and below an axis, with a cursor and a hover.

use std::ops::Range;

use canopy::{
    Context, EventOutcome, NodeName, Register, Setup, ViewContext, Widget,
    commands::CommandCall,
    derive_commands,
    error::Result,
    geom::{Line, PointI32, Rect, Size},
    input::{Event, mouse},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
    text,
};

use crate::chart::{self, Base, Column, Marker, Scale, Tint};

/// Rows below the axis by default.
const LOWER_ROWS: u32 = 3;

/// Rows of the chart besides its two areas: the marker lane and the axis.
const FRAME_ROWS: u32 = 2;

/// Rows above the axis that a chart measures when nothing constrains it.
const UPPER_ROWS: u32 = 6;

/// Rows above the axis that a short chart keeps before it gives rows below
/// the axis.
const MIN_UPPER_ROWS: u32 = 2;

/// Rows above the axis from which the gutter shows the middle of the scale.
const MIDDLE_LABEL_ROWS: u32 = 5;

/// Light of the cursor column.
const CURSOR_LIGHT: f32 = 0.4;

/// Mute of a muted column.
const MUTE: f32 = 0.8;

/// Glyph of the cursor in the axis.
const CURSOR: char = '▲';

/// Glyph of the hover in the axis.
const HOVER: char = '△';

/// Glyph of a reference line.
const REFERENCE: char = '┄';

/// Glyph of the axis.
const AXIS: char = '─';

/// Formats a value of the scale for the gutter.
type Format = Box<dyn Fn(f64) -> String>;

/// Stacked columns above and below one axis.
///
/// Each [`Column`] has a stack that grows up from the axis on the upper scale
/// and a stack that grows down from it on the lower scale. A dotted reference
/// line marks a value above the axis, such as a limit, in the cells that the
/// stack leaves empty. A lane above the chart holds a marker glyph for each
/// column. The axis row holds labels at the columns that they name, `▲` at the
/// cursor, and `△` at the hover. A gutter on the left shows the scale.
///
/// The chart shows one column a cell. When the columns outnumber the cells,
/// it scrolls to keep the cursor in view, and it shows the newest columns
/// when it has no cursor. With [`ColumnChart::set_fit`], it instead fits every
/// column to the width: each cell shows a run of consecutive columns. The
/// upper area of the cell draws the member with the largest upper stack, and
/// the lower area the member with the largest lower stack, both from the
/// members that are not muted when the run has any. The marker is the member
/// marker of the highest rank, and the cell is muted when every member is.
/// [`ColumnChart::group`] returns the run of a column, and
/// [`ColumnChart::drawn_upper`] the member whose upper stack draws.
///
/// The cursor column draws lighter, and a muted column draws toward the
/// ground. A click sets the cursor. A pointer move sets the hover, and the
/// pointer leaving the chart clears it. A key move,
/// [`ColumnChart::set_columns`], and a change of the fit clear it too. The
/// wheel moves the cursor. Moving the cursor to the newest column turns on
/// following: after [`ColumnChart::set_columns`], the cursor stays on the
/// newest column. [`ColumnChart::with_command`] sets a call that runs after
/// each change of the cursor or the hover by input or by a cursor command.
///
/// `ColumnChart` pushes the `column_chart` layer. It paints the `axis`,
/// `label`, `reference`, and `cursor` parts, and each marker and segment in
/// its own style path.
pub struct ColumnChart {
    /// The columns, oldest first.
    columns: Vec<Column>,
    /// Scale of the stacks above the axis.
    upper: Scale,
    /// Scale of the stacks below the axis.
    lower: Scale,
    /// Axis labels: the column that each names, and its text.
    labels: Vec<(usize, String)>,
    /// Cursor column.
    cursor: Option<usize>,
    /// Hover column.
    hover: Option<usize>,
    /// Whether the cursor stays on the newest column.
    following: bool,
    /// Whether every column fits the width.
    fit: bool,
    /// First column in view, when the chart scrolls.
    offset: usize,
    /// The columns of each cell, left to right, for the cells and the
    /// columns of the chart now.
    slots: Vec<Range<usize>>,
    /// Cells for columns at the last render.
    cells: usize,
    /// Width of the gutter at the last render.
    gutter: u32,
    /// Rows below the axis.
    lower_rows: u32,
    /// Whether the gutter shows the scale.
    scale_labels: bool,
    /// Text of a scale value.
    format: Format,
    /// Call that runs after a change of the cursor or the hover.
    command: Option<CommandCall>,
}

impl Default for ColumnChart {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl ColumnChart {
    /// Constructs a chart without columns, with three rows below the axis.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            upper: Scale::linear(1.0),
            lower: Scale::linear(1.0),
            labels: Vec::new(),
            cursor: None,
            hover: None,
            following: false,
            fit: false,
            offset: 0,
            slots: Vec::new(),
            cells: 0,
            gutter: 0,
            lower_rows: LOWER_ROWS,
            scale_labels: true,
            format: Box::new(|value| format!("{value}")),
            command: None,
        }
    }

    /// Sets the rows below the axis. Zero rows leave only the upper stacks.
    #[must_use]
    pub fn with_lower_rows(mut self, rows: u32) -> Self {
        self.lower_rows = rows;
        self
    }

    /// Sets the rows below the axis. Zero rows leave only the upper stacks.
    pub fn set_lower_rows(&mut self, rows: u32) {
        self.lower_rows = rows;
    }

    /// Shows or hides the scale in the gutter.
    #[must_use]
    pub fn with_scale_labels(mut self, shown: bool) -> Self {
        self.scale_labels = shown;
        self
    }

    /// Shows or hides the scale in the gutter.
    pub fn set_scale_labels(&mut self, shown: bool) {
        self.scale_labels = shown;
    }

    /// Sets the text of the values in the gutter.
    #[must_use]
    pub fn with_format(mut self, format: impl Fn(f64) -> String + 'static) -> Self {
        self.format = Box::new(format);
        self
    }

    /// Sets a call that runs after each change of the cursor or the hover by
    /// input or by a cursor command. The setters do not post it. The chart
    /// posts the call, so it runs once the handler of the chart returns.
    #[must_use]
    pub fn with_command(mut self, call: CommandCall) -> Self {
        self.command = Some(call);
        self
    }

    /// Fits every column to the width, or shows one column a cell and
    /// scrolls. A change of the fit clears the hover, whose cell now shows
    /// other columns.
    pub fn set_fit(&mut self, fit: bool) {
        if self.fit != fit {
            self.hover = None;
        }
        self.fit = fit;
        self.lay_out();
    }

    /// Returns whether every column fits the width.
    pub fn fit(&self) -> bool {
        self.fit
    }

    /// Replaces the columns and their scales, and clears the hover. With
    /// following on, the cursor moves to the newest column. Otherwise it
    /// stays at its index, within the new columns.
    pub fn set_columns(&mut self, columns: Vec<Column>, upper: Scale, lower: Scale) {
        self.columns = columns;
        self.upper = upper;
        self.lower = lower;
        self.hover = None;
        let last = self.columns.len().checked_sub(1);
        self.cursor = if self.following {
            last
        } else {
            self.cursor
                .and_then(|cursor| last.map(|last| cursor.min(last)))
        };
        self.lay_out();
    }

    /// Replaces the axis labels: the column that each names, and its text.
    pub fn set_labels(&mut self, labels: Vec<(usize, String)>) {
        self.labels = labels;
    }

    /// Returns the columns.
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Places the cursor on a column, or removes it. Following turns on when
    /// the cursor is on the newest column.
    pub fn set_cursor(&mut self, cursor: Option<usize>) {
        let last = self.columns.len().checked_sub(1);
        self.cursor = cursor.and_then(|cursor| last.map(|last| cursor.min(last)));
        self.following = self.cursor.is_some() && self.cursor == last;
        self.lay_out();
    }

    /// Returns the cursor column.
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    /// Returns the hover column.
    pub fn hover(&self) -> Option<usize> {
        self.hover
    }

    /// Clears the hover.
    pub fn clear_hover(&mut self) {
        self.hover = None;
    }

    /// Returns whether the cursor follows the newest column.
    pub fn following(&self) -> bool {
        self.following
    }

    /// Returns the columns in view: the first, and how many.
    #[cfg(test)]
    fn in_view(&self) -> (usize, usize) {
        match (self.slots.first(), self.slots.last()) {
            (Some(first), Some(last)) => (first.start, last.end - first.start),
            _ => (0, 0),
        }
    }

    /// Returns the columns that share the cell of `column`: the column alone,
    /// or its run when the columns fit the width. Before the first render has
    /// established the cell width, there is no group yet.
    pub fn group(&self, column: usize) -> Option<Range<usize>> {
        if self.cells == 0 || column >= self.columns.len() {
            return None;
        }
        Some(
            self.slots
                .iter()
                .find(|slot| slot.contains(&column))
                .cloned()
                .unwrap_or(column..column + 1),
        )
    }

    /// Returns the column whose upper stack the cell of `column` draws: the
    /// column itself, or one member of its run when the columns fit the
    /// width.
    pub fn drawn_upper(&self, column: usize) -> Option<usize> {
        self.group(column).map(|group| self.drawn(&group).upper)
    }

    /// Moves the cursor by `delta` cells: one column, or one run of columns
    /// when they fit the width. A chart without a cursor starts from the
    /// newest column.
    #[command]
    pub fn cursor_by(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        let Some(last) = self.columns.len().checked_sub(1) else {
            return Ok(());
        };
        let from = self.cursor.unwrap_or(last);
        let fitted = self.fit && self.columns.len() > self.cells;
        let to = match self.slots.iter().position(|slot| slot.contains(&from)) {
            Some(slot) if fitted => {
                let target = slot
                    .saturating_add_signed(delta as isize)
                    .min(self.slots.len() - 1);
                if target == slot {
                    from
                } else {
                    self.slots[target].end - 1
                }
            }
            _ => from.saturating_add_signed(delta as isize).min(last),
        };
        self.move_to(ctx, to)
    }

    /// Moves the cursor to the next label, or to the previous one when
    /// `delta` is negative.
    #[command]
    pub fn cursor_label(&mut self, ctx: &mut dyn Context, delta: i32) -> Result<()> {
        let Some(last) = self.columns.len().checked_sub(1) else {
            return Ok(());
        };
        let from = self.cursor.unwrap_or(last);
        let mut columns = self
            .labels
            .iter()
            .map(|(column, _)| *column)
            .filter(|column| *column <= last)
            .collect::<Vec<_>>();
        columns.sort_unstable();
        let to = if delta >= 0 {
            columns.into_iter().find(|column| *column > from)
        } else {
            columns.into_iter().rev().find(|column| *column < from)
        };
        match to {
            Some(to) => self.move_to(ctx, to),
            None => Ok(()),
        }
    }

    /// Moves the cursor to the first column.
    #[command]
    pub fn cursor_first(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.columns.is_empty() {
            return Ok(());
        }
        self.move_to(ctx, 0)
    }

    /// Moves the cursor to the newest column, which it then follows.
    #[command]
    pub fn cursor_newest(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(last) = self.columns.len().checked_sub(1) else {
            return Ok(());
        };
        self.move_to(ctx, last)
    }

    /// Moves the cursor to one column, and clears the hover.
    #[command]
    pub fn cursor_to(&mut self, ctx: &mut dyn Context, column: usize) -> Result<()> {
        if self.columns.is_empty() {
            return Ok(());
        }
        self.move_to(ctx, column)
    }

    /// Moves the cursor to one column by a key or the wheel, and clears the
    /// hover.
    fn move_to(&mut self, ctx: &mut dyn Context, column: usize) -> Result<()> {
        let before = (self.cursor, self.hover);
        self.set_cursor(Some(column));
        self.hover = None;
        self.changed(ctx, before)
    }

    /// Posts the command when the cursor or the hover changed from `before`.
    fn changed(&self, ctx: &mut dyn Context, before: (Option<usize>, Option<usize>)) -> Result<()> {
        if before == (self.cursor, self.hover) {
            return Ok(());
        }
        match &self.command {
            Some(call) => ctx.post(call),
            None => Ok(()),
        }
    }

    /// Returns the width of the gutter for `width` columns of cells: the
    /// widest scale label and a space, or nothing when the labels are off or
    /// leave no room for the chart.
    fn gutter(&self, width: u32, rows: &Rows) -> u32 {
        if !self.scale_labels {
            return 0;
        }
        let labels = self.scale_texts(rows);
        let widest = labels
            .iter()
            .map(|(_, label)| text::width(label))
            .max()
            .unwrap_or(0);
        let gutter = widest.saturating_add(1);
        if gutter < width { gutter } else { 0 }
    }

    /// Returns the scale labels of the gutter: the row of each, and its text.
    fn scale_texts(&self, rows: &Rows) -> Vec<(u32, String)> {
        let mut labels = vec![
            (rows.upper_top(), (self.format)(self.upper.max())),
            (rows.axis(), (self.format)(0.0)),
        ];
        if rows.upper >= MIDDLE_LABEL_ROWS {
            labels.push((
                rows.upper_top() + rows.upper / 2,
                (self.format)(self.upper.max() / 2.0),
            ));
        }
        if rows.lower > 0 {
            labels.push((rows.axis() + rows.lower, (self.format)(self.lower.max())));
        }
        labels
    }

    /// Returns the column under a widget-local location: the column of its
    /// cell, or the newest column of the run of its cell.
    fn column_at(&self, ctx: &dyn ViewContext, location: PointI32) -> Option<usize> {
        let point = ctx.view().viewport_point(location)?;
        let x = point.x.checked_sub(self.gutter)?;
        self.slots.get(x as usize).map(|slot| slot.end - 1)
    }

    /// Lays out the columns on the cells of the last render: runs that fit
    /// the width, or one column a cell from an offset that keeps the cursor
    /// in view.
    fn lay_out(&mut self) {
        let cells = self.cells;
        let count = self.columns.len();
        if cells == 0 {
            self.slots.clear();
            return;
        }
        if self.fit && count > cells {
            self.offset = 0;
            self.slots = (0..cells)
                .map(|cell| cell * count / cells..(cell + 1) * count / cells)
                .collect();
            return;
        }
        let latest = count.saturating_sub(cells);
        self.offset = match self.cursor {
            None => latest,
            Some(cursor) if cursor < self.offset => cursor,
            Some(cursor) if cursor >= self.offset + cells => cursor + 1 - cells,
            Some(_) => self.offset.min(latest),
        };
        let end = (self.offset + cells).min(count);
        self.slots = (self.offset..end)
            .map(|column| column..column + 1)
            .collect();
    }

    /// Returns the cell of a column at the last layout.
    fn cell_of(&self, column: usize) -> Option<usize> {
        self.slots.iter().position(|slot| slot.contains(&column))
    }

    /// Returns the column that stands for a run of columns in one cell: the
    /// run and the part it draws.
    fn drawn(&self, slot: &Range<usize>) -> Drawn {
        let members = &self.columns[slot.clone()];
        let shown = members.iter().any(|column| !column.muted);
        // The members that are not muted draw, when the run has any.
        let candidates = || {
            slot.clone()
                .zip(members)
                .filter(move |(_, column)| !shown || !column.muted)
        };
        let largest = |stack: fn(&Column) -> &[chart::Segment]| {
            candidates()
                .max_by(|(_, a), (_, b)| height(stack(a)).total_cmp(&height(stack(b))))
                .map_or(slot.start, |(index, _)| index)
        };
        let marker = members
            .iter()
            .filter_map(|column| column.marker.as_ref())
            .max_by_key(|marker| marker.rank)
            .cloned();
        Drawn {
            upper: largest(|column| &column.upper),
            lower: largest(|column| &column.lower),
            marker,
            muted: !shown,
        }
    }

    /// Paints the axis row: the rule, the labels in view, and the cursor and
    /// the hover.
    fn paint_axis(&self, render: &mut Render<'_>, x: u32, y: u32, width: u32) -> Result<()> {
        let rule = AXIS.to_string().repeat(width as usize);
        render.text("axis", Line::new(x, y, width), &rule)?;
        let cursor = self.cursor.and_then(|cursor| self.cell_of(cursor));
        let hover = self.hover.and_then(|hover| self.cell_of(hover));
        let marks = [cursor, hover].into_iter().flatten().collect::<Vec<_>>();
        // A label needs one blank cell after the one before it, and gives
        // way to the cursor and the hover.
        let mut free = 0;
        let mut labels = self.labels.iter().collect::<Vec<_>>();
        labels.sort_by_key(|(column, _)| *column);
        for (column, label) in labels {
            let Some(at) = self.cell_of(*column) else {
                continue;
            };
            let label_width = text::width(label);
            let span = at..at + label_width as usize;
            let at = at as u32;
            if at < free
                || at.saturating_add(label_width) > width
                || marks.iter().any(|mark| span.contains(mark))
            {
                continue;
            }
            render.text("label", Line::new(x + at, y, label_width), label)?;
            free = at + label_width + 1;
        }
        if let Some(at) = hover
            && Some(at) != cursor
        {
            render.text("cursor", Line::new(x + at as u32, y, 1), &HOVER.to_string())?;
        }
        if let Some(at) = cursor {
            render.text(
                "cursor",
                Line::new(x + at as u32, y, 1),
                &CURSOR.to_string(),
            )?;
        }
        Ok(())
    }

    /// Paints the reference line of one column in the cell of its value, when
    /// its stack leaves the cell empty.
    fn paint_reference(
        &self,
        render: &mut Render<'_>,
        column: &Column,
        x: u32,
        top: u32,
        rows: &Rows,
    ) -> Result<()> {
        let Some(value) = column.reference else {
            return Ok(());
        };
        if !(value > 0.0 && value <= self.upper.max()) {
            return Ok(());
        }
        let eighths = self.upper.eighths(value, rows.upper);
        let Some(row) = eighths.checked_sub(1).map(|eighth| eighth / 8) else {
            return Ok(());
        };
        if self.upper.eighths(height(&column.upper), rows.upper) > row * 8 {
            return Ok(());
        }
        let y = top + rows.axis() - 1 - row;
        render.text("reference", Line::new(x, y, 1), &REFERENCE.to_string())
    }
}

/// What one cell draws for its run of columns.
struct Drawn {
    /// Column whose upper stack and reference draw.
    upper: usize,
    /// Column whose lower stack draws.
    lower: usize,
    /// Marker of the highest rank.
    marker: Option<Marker>,
    /// Whether every member is muted.
    muted: bool,
}

/// Returns the height of a stack: the sum of its values above zero.
fn height(segments: &[chart::Segment]) -> f64 {
    segments
        .iter()
        .map(|segment| segment.value)
        .filter(|value| *value > 0.0)
        .sum()
}

/// The rows of a chart.
struct Rows {
    /// Rows above the axis.
    upper: u32,
    /// Rows below the axis.
    lower: u32,
}

impl Rows {
    /// Returns the rows of a chart `height` rows high, or `None` when the
    /// chart has no room for one row above the axis. The rows below the axis
    /// give way first, down to two rows above it.
    fn of(height: u32, lower: u32) -> Option<Self> {
        height.checked_sub(FRAME_ROWS + 1)?;
        let lower = lower.min(height.saturating_sub(FRAME_ROWS + MIN_UPPER_ROWS));
        Some(Self {
            upper: height - FRAME_ROWS - lower,
            lower,
        })
    }

    /// Returns the top row above the axis.
    fn upper_top(&self) -> u32 {
        1
    }

    /// Returns the axis row.
    fn axis(&self) -> u32 {
        1 + self.upper
    }
}

impl Register for ColumnChart {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}

impl Widget for ColumnChart {
    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let width = u32::try_from(self.columns.len()).unwrap_or(u32::MAX);
        c.clamp(Size::new(
            width.saturating_add(6),
            FRAME_ROWS + UPPER_ROWS + self.lower_rows,
        ))
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("column_chart");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill("", area, ' ')?;
        let Some(rows) = Rows::of(area.h, self.lower_rows) else {
            self.cells = 0;
            self.slots.clear();
            return Ok(());
        };
        let gutter = self.gutter(area.w, &rows);
        let width = area.w - gutter;
        self.gutter = gutter;
        self.cells = width as usize;
        self.lay_out();
        let (x0, y0) = (area.tl.x, area.tl.y);
        if gutter > 0 {
            for (row, label) in self.scale_texts(&rows) {
                let label_width = text::width(&label);
                let x = x0 + (gutter - 1).saturating_sub(label_width);
                render.text("label", Line::new(x, y0 + row, label_width), &label)?;
            }
        }
        let plot = x0 + gutter;
        let cursor = self.cursor.and_then(|cursor| self.cell_of(cursor));
        for (at, slot) in self.slots.iter().enumerate() {
            let drawn = self.drawn(slot);
            let x = plot + at as u32;
            let tint = if Some(at) == cursor {
                Tint::Light(CURSOR_LIGHT)
            } else if drawn.muted {
                Tint::Mute(MUTE)
            } else {
                Tint::None
            };
            let upper = &self.columns[drawn.upper];
            let area = Rect::new(x, y0 + rows.upper_top(), 1, rows.upper);
            chart::column(render, area, Base::Bottom, &self.upper, &upper.upper, tint)?;
            self.paint_reference(render, upper, x, y0, &rows)?;
            if rows.lower > 0 {
                let lower = &self.columns[drawn.lower];
                let area = Rect::new(x, y0 + rows.axis() + 1, 1, rows.lower);
                chart::column(render, area, Base::Top, &self.lower, &lower.lower, tint)?;
            }
            if let Some(marker) = &drawn.marker {
                render.text(
                    &marker.style,
                    Line::new(x, y0, 1),
                    &marker.glyph.to_string(),
                )?;
            }
        }
        self.paint_axis(render, plot, y0 + rows.axis(), width)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        let Event::Mouse(m) = event else {
            return Ok(EventOutcome::Ignore);
        };
        let before = (self.cursor, self.hover);
        match m.action {
            mouse::Action::Down if m.button == mouse::Button::Left => {
                let Some(column) = self.column_at(ctx, m.location) else {
                    return Ok(EventOutcome::Ignore);
                };
                self.set_cursor(Some(column));
            }
            mouse::Action::Moved => self.hover = self.column_at(ctx, m.location),
            mouse::Action::Leave => self.hover = None,
            mouse::Action::ScrollUp | mouse::Action::ScrollLeft => {
                self.cursor_by(ctx, -1)?;
                return Ok(EventOutcome::Handle);
            }
            mouse::Action::ScrollDown | mouse::Action::ScrollRight => {
                self.cursor_by(ctx, 1)?;
                return Ok(EventOutcome::Handle);
            }
            _ => return Ok(EventOutcome::Ignore),
        }
        self.changed(ctx, before)?;
        Ok(EventOutcome::Handle)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("column_chart")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        ContextExt,
        commands::CommandTarget,
        geom::{Point, PointI32},
        input::key,
        runtime::TurnInput,
        style::{Color, ResolvedStyle},
        testing::harness::Harness,
    };

    use super::*;
    use crate::chart::{Marker, Segment};

    /// Color of segment `a`.
    const A: Color = Color::Rgb { r: 200, g: 0, b: 0 };
    /// Color of segment `b`.
    const B: Color = Color::Rgb { r: 0, g: 200, b: 0 };

    /// Root that mounts a chart and counts the changes that it posts.
    struct Owner {
        /// The chart to mount.
        chart: Option<ColumnChart>,
        /// Changes posted so far.
        changes: usize,
    }

    #[derive_commands]
    impl Owner {
        /// Counts one change of the cursor or the hover.
        #[command]
        fn moved(&mut self) {
            self.changes += 1;
        }
    }

    impl Widget for Owner {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
            render.fill("", ctx.view().view_rect_local(), ' ')
        }

        fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
            let owner = CommandTarget::Exact(c.node_id());
            let chart = self
                .chart
                .take()
                .expect("chart")
                .with_command(Self::spec_moved().call().with_target(owner));
            let chart = c.add_child(c.node_id(), chart)?;
            c.set_layout_override(chart.into(), Layout::fill().into())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("owner")
        }
    }

    impl Register for Owner {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<ColumnChart>()?;
            setup.add_commands::<Self>()
        }
    }

    /// Returns a harness of `chart` on a `width` × `height` screen, with the
    /// test colors.
    fn harness(chart: ColumnChart, width: u32, height: u32) -> Result<Harness> {
        let mut harness = Harness::builder(Owner {
            chart: Some(chart),
            changes: 0,
        })
        .register::<Owner>()
        .size(width, height)
        .configure(|setup| {
            setup.widget_styles(|_palette, rules| {
                rules.fg("a", A).fg("b", B).apply();
            });
            Ok(())
        })
        .build()?;
        harness.render()?;
        Ok(harness)
    }

    /// Runs `f` with the chart and its context, then renders.
    fn chart<R>(
        harness: &mut Harness,
        f: impl FnOnce(&mut ColumnChart, &mut dyn Context) -> Result<R>,
    ) -> Result<R> {
        let result = harness.with_unique(f)?;
        harness.render()?;
        Ok(result)
    }

    /// Returns the changes that the chart posted.
    fn changes(harness: &mut Harness) -> usize {
        harness.with_root_widget(|owner: &mut Owner| owner.changes)
    }

    /// Returns the rows on screen, without trailing spaces.
    fn screen(harness: &Harness) -> Vec<String> {
        harness
            .tbuf()
            .lines()
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    /// Returns the glyph and the style of one cell.
    fn cell(harness: &Harness, x: u32, y: u32) -> (char, ResolvedStyle) {
        let cell = harness.buf().get(Point { x, y }).expect("cell");
        (cell.ch, cell.style)
    }

    /// Returns a column with one upper and one lower segment.
    fn bar(upper: f64, lower: f64) -> Column {
        Column {
            upper: vec![Segment::new(upper, "a")],
            lower: vec![Segment::new(lower, "b")],
            ..Column::default()
        }
    }

    /// Returns `count` columns of one value each.
    fn bars(count: usize) -> Vec<Column> {
        (0..count).map(|_| bar(8.0, 8.0)).collect()
    }

    #[test]
    fn groups_need_a_rendered_cell_width() {
        let mut chart = ColumnChart::new();
        chart.set_columns(bars(8), Scale::linear(8.0), Scale::linear(8.0));
        chart.set_fit(true);
        assert_eq!(chart.group(7), None);
        assert_eq!(chart.drawn_upper(7), None);
    }

    /// Sends one mouse event at a screen cell.
    fn mouse(harness: &mut Harness, action: mouse::Action, x: i32, y: i32) -> Result<()> {
        let button = match action {
            mouse::Action::Down => mouse::Button::Left,
            _ => mouse::Button::None,
        };
        harness.mouse(mouse::MouseEvent {
            action,
            button,
            modifiers: key::Empty,
            location: PointI32 { x, y },
        })
    }

    #[test]
    fn columns_stack_up_and_down_from_the_axis() -> Result<()> {
        // Seven rows: the marker lane, three rows above the axis, the axis,
        // and two rows below it.
        let mut harness = harness(
            ColumnChart::new()
                .with_lower_rows(2)
                .with_scale_labels(false),
            4,
            7,
        )?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(
                vec![bar(24.0, 16.0), bar(12.0, 4.0), bar(0.0, 0.0)],
                Scale::linear(24.0),
                Scale::linear(16.0),
            );
            Ok(())
        })?;
        assert_eq!(
            screen(&harness),
            ["", "█", "█▄", "██", "────", "█▄", "█"],
            "12 of 24 fills a row and a half, and 4 of 16 half a row"
        );
        // A downward half row is a lower half block in swapped colors.
        let (glyph, style) = cell(&harness, 1, 5);
        assert_eq!(glyph, '▄');
        assert_eq!(style.bg, B);
        assert_eq!(cell(&harness, 0, 1).1.fg, A);
        assert_eq!(cell(&harness, 0, 5).1.fg, B);
        Ok(())
    }

    #[test]
    fn the_gutter_shows_the_scale() -> Result<()> {
        let chart_widget = ColumnChart::new().with_format(|value| format!("{value:.0}"));
        let mut harness = harness(chart_widget, 8, 10)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(
                vec![bar(24.0, 16.0)],
                Scale::linear(24.0),
                Scale::linear(16.0),
            );
            Ok(())
        })?;
        let rows = screen(&harness);
        // Five rows above the axis show the top, the middle, and zero.
        assert_eq!(rows[1], "24 █");
        assert_eq!(rows[3], "12 █");
        assert_eq!(rows[6], " 0 ─────");
        assert_eq!(rows[9], "16 █");
        chart(&mut harness, |chart, _| {
            chart.set_scale_labels(false);
            Ok(())
        })?;
        assert_eq!(screen(&harness)[6], "────────");
        Ok(())
    }

    #[test]
    fn a_short_chart_gives_up_the_lower_rows_first() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 2, 4)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(vec![bar(8.0, 8.0)], Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        assert_eq!(screen(&harness), ["", "█", "█", "──"]);
        let mut tiny = harness_of_height(2)?;
        assert!(screen(&tiny).iter().all(String::is_empty), "no room");
        tiny.render()?;
        Ok(())
    }

    /// Returns a chart harness `height` rows high with one column.
    fn harness_of_height(height: u32) -> Result<Harness> {
        let mut harness = harness(ColumnChart::new(), 4, height)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(vec![bar(8.0, 8.0)], Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        Ok(harness)
    }

    #[test]
    fn a_reference_line_marks_empty_cells_below_the_top() -> Result<()> {
        let reference = |upper: f64, reference: f64| Column {
            reference: Some(reference),
            ..bar(upper, 0.0)
        };
        let mut harness = harness(
            ColumnChart::new()
                .with_lower_rows(0)
                .with_scale_labels(false),
            4,
            5,
        )?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(
                vec![
                    reference(4.0, 20.0),
                    reference(24.0, 20.0),
                    reference(4.0, 30.0),
                    reference(4.0, 4.0),
                ],
                Scale::linear(24.0),
                Scale::linear(1.0),
            );
            Ok(())
        })?;
        let rows = screen(&harness);
        assert_eq!(rows[1], "┄█", "the full column hides its line");
        assert_eq!(rows[3], "▄█▄▄", "a line inside the stack hides");
        assert!(!rows.concat().contains("┄┄"), "a line past the top hides");
        Ok(())
    }

    #[test]
    fn markers_sit_in_the_lane_above_their_columns() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 3, 8)?;
        chart(&mut harness, |chart, _| {
            let marked = |glyph| Column {
                marker: Some(Marker::new(glyph, "a")),
                ..bar(1.0, 1.0)
            };
            chart.set_columns(
                vec![marked('✕'), bar(1.0, 1.0), marked('◆')],
                Scale::linear(8.0),
                Scale::linear(8.0),
            );
            Ok(())
        })?;
        assert_eq!(screen(&harness)[0], "✕ ◆");
        assert_eq!(cell(&harness, 0, 0).1.fg, A, "the marker takes its style");
        Ok(())
    }

    #[test]
    fn labels_show_where_they_fit_without_touching() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 12, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(12), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_labels(vec![
                (0, "10:00".to_owned()),
                (5, "x".to_owned()),
                (6, "y".to_owned()),
                (10, "late".to_owned()),
            ]);
            Ok(())
        })?;
        // Six rows: the lane, two rows above the axis, and two below it.
        let axis = &screen(&harness)[3];
        assert_eq!(
            axis, "10:00─y─────",
            "x touches the first label, late runs out"
        );
        chart(&mut harness, |chart, _| {
            chart.set_cursor(Some(2));
            Ok(())
        })?;
        assert_eq!(
            screen(&harness)[3],
            "──▲──x──────",
            "a label under the cursor gives way to the next one"
        );
        Ok(())
    }

    #[test]
    fn cursor_commands_move_the_cursor_and_follow_the_newest() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 10, 6)?;
        chart(&mut harness, |chart, ctx| {
            chart.set_columns(bars(5), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_labels(vec![(1, "a".to_owned()), (3, "b".to_owned())]);
            chart.cursor_by(ctx, -1)?;
            assert_eq!(chart.cursor(), Some(3), "a first move starts at the newest");
            assert!(!chart.following());
            chart.cursor_label(ctx, -1)?;
            assert_eq!(chart.cursor(), Some(1));
            chart.cursor_label(ctx, -1)?;
            assert_eq!(chart.cursor(), Some(1), "no label before the first");
            chart.cursor_label(ctx, 1)?;
            assert_eq!(chart.cursor(), Some(3));
            chart.cursor_newest(ctx)?;
            assert_eq!(chart.cursor(), Some(4));
            assert!(chart.following());
            chart.set_columns(bars(7), Scale::linear(8.0), Scale::linear(8.0));
            assert_eq!(chart.cursor(), Some(6), "following keeps the newest");
            chart.cursor_first(ctx)?;
            assert!(!chart.following());
            chart.set_columns(bars(9), Scale::linear(8.0), Scale::linear(8.0));
            assert_eq!(chart.cursor(), Some(0), "the cursor stays put");
            chart.set_columns(Vec::new(), Scale::linear(8.0), Scale::linear(8.0));
            assert_eq!(chart.cursor(), None);
            chart.cursor_by(ctx, 1)?;
            assert_eq!(chart.cursor(), None, "no columns, no cursor");
            Ok(())
        })?;
        harness.render()?;
        assert_eq!(
            changes(&mut harness),
            5,
            "the moves that changed the cursor"
        );
        Ok(())
    }

    #[test]
    fn the_axis_marks_the_cursor_and_the_hover() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 6, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(6), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_cursor(Some(1));
            Ok(())
        })?;
        mouse(&mut harness, mouse::Action::Moved, 4, 2)?;
        harness.render()?;
        assert_eq!(screen(&harness)[3], "─▲──△─");
        assert_eq!(changes(&mut harness), 1);
        Ok(())
    }

    #[test]
    fn the_chart_scrolls_to_keep_the_cursor_in_view() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 5, 6)?;
        let view = |harness: &mut Harness| {
            harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.in_view()))
        };
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(20), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        assert_eq!(
            view(&mut harness)?,
            (15, 5),
            "the newest columns without a cursor"
        );
        chart(&mut harness, |chart, _| {
            chart.set_cursor(Some(2));
            Ok(())
        })?;
        assert_eq!(view(&mut harness)?, (2, 5));
        chart(&mut harness, |chart, ctx| chart.cursor_by(ctx, 4))?;
        assert_eq!(
            view(&mut harness)?,
            (2, 5),
            "a cursor in view does not scroll"
        );
        chart(&mut harness, |chart, ctx| chart.cursor_by(ctx, 1))?;
        assert_eq!(
            view(&mut harness)?,
            (3, 5),
            "one step past the edge scrolls one"
        );
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(4), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        assert_eq!(view(&mut harness)?, (0, 4), "fewer columns than cells");
        Ok(())
    }

    #[test]
    fn a_click_sets_the_cursor_and_the_wheel_moves_it() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 8, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(8), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        let cursor = |harness: &mut Harness| {
            harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.cursor()))
        };
        mouse(&mut harness, mouse::Action::Down, 2, 3)?;
        assert_eq!(cursor(&mut harness)?, Some(2));
        mouse(&mut harness, mouse::Action::ScrollDown, 2, 3)?;
        assert_eq!(cursor(&mut harness)?, Some(3));
        mouse(&mut harness, mouse::Action::ScrollUp, 2, 3)?;
        mouse(&mut harness, mouse::Action::ScrollUp, 2, 3)?;
        assert_eq!(cursor(&mut harness)?, Some(1));
        mouse(&mut harness, mouse::Action::Down, 7, 3)?;
        let following = harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.following()))?;
        assert!(following, "a click on the newest column follows it");
        Ok(())
    }

    #[test]
    fn the_hover_clears_on_leave_on_keys_and_on_new_columns() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 8, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(8), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        let hover = |harness: &mut Harness| {
            harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.hover()))
        };
        mouse(&mut harness, mouse::Action::Moved, 3, 1)?;
        assert_eq!(hover(&mut harness)?, Some(3));
        harness
            .canopy
            .turn(TurnInput::Events(vec![Event::FocusLost]))?;
        assert_eq!(hover(&mut harness)?, None, "leave clears it");
        mouse(&mut harness, mouse::Action::Moved, 4, 1)?;
        chart(&mut harness, |chart, ctx| chart.cursor_by(ctx, -1))?;
        assert_eq!(hover(&mut harness)?, None, "a key move clears it");
        mouse(&mut harness, mouse::Action::Moved, 5, 1)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(8), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        assert_eq!(hover(&mut harness)?, None, "new columns clear it");
        Ok(())
    }

    #[test]
    fn the_cursor_column_lightens_and_a_muted_column_fades() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 3, 6)?;
        chart(&mut harness, |chart, _| {
            let muted = Column {
                muted: true,
                ..bar(8.0, 8.0)
            };
            chart.set_columns(
                vec![bar(8.0, 8.0), bar(8.0, 8.0), muted],
                Scale::linear(8.0),
                Scale::linear(8.0),
            );
            chart.set_cursor(Some(1));
            Ok(())
        })?;
        let plain = cell(&harness, 0, 1).1.fg;
        let lit = cell(&harness, 1, 1).1.fg;
        let faded = cell(&harness, 2, 1).1.fg;
        assert_eq!(plain, A);
        assert_ne!(lit, A, "the cursor column lightens");
        assert_ne!(faded, A, "a muted column fades");
        assert_ne!(lit, faded);
        Ok(())
    }

    /// Returns a column of one upper value, muted or not, with a marker of
    /// `rank` when one is given.
    fn member(upper: f64, muted: bool, marker: Option<(char, u8)>) -> Column {
        Column {
            upper: vec![Segment::new(upper, "a")],
            lower: vec![Segment::new(upper, "b")],
            marker: marker.map(|(glyph, rank)| Marker::new(glyph, "a").with_rank(rank)),
            muted,
            ..Column::default()
        }
    }

    #[test]
    fn fitted_columns_draw_the_largest_member_of_each_run() -> Result<()> {
        let mut harness = harness(
            ColumnChart::new()
                .with_lower_rows(0)
                .with_scale_labels(false),
            4,
            4,
        )?;
        chart(&mut harness, |chart, _| {
            // Eight columns on four cells: two a cell.
            let heights = [2.0, 16.0, 8.0, 4.0, 0.0, 0.0, 16.0, 16.0];
            chart.set_columns(
                heights
                    .iter()
                    .map(|value| member(*value, false, None))
                    .collect(),
                Scale::linear(16.0),
                Scale::linear(16.0),
            );
            chart.set_fit(true);
            Ok(())
        })?;
        assert_eq!(screen(&harness), ["", "█  █", "██ █", "────"]);
        let (group, view) = harness
            .with_unique(|chart: &mut ColumnChart, _| Ok((chart.group(3), chart.in_view())))?;
        assert_eq!(group, Some(2..4));
        assert_eq!(view, (0, 8), "every column is in view");
        chart(&mut harness, |chart, _| {
            chart.set_fit(false);
            Ok(())
        })?;
        let view = harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.in_view()))?;
        assert_eq!(view, (4, 4), "one column a cell scrolls to the newest");
        Ok(())
    }

    #[test]
    fn a_fitted_run_prefers_members_that_are_not_muted_and_its_highest_marker() -> Result<()> {
        let mut harness = harness(
            ColumnChart::new()
                .with_lower_rows(0)
                .with_scale_labels(false),
            2,
            4,
        )?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(
                vec![
                    member(16.0, true, Some(('✕', 7))),
                    member(4.0, false, Some(('↻', 1))),
                    member(16.0, true, None),
                    member(8.0, true, Some(('◆', 2))),
                ],
                Scale::linear(16.0),
                Scale::linear(16.0),
            );
            chart.set_fit(true);
            Ok(())
        })?;
        let rows = screen(&harness);
        assert_eq!(rows[0], "✕◆", "the highest rank of each run");
        assert_eq!(rows[1], " █", "the first run draws its short member");
        assert_eq!(rows[2], "▄█");
        assert_eq!(
            cell(&harness, 0, 2).1.fg,
            A,
            "a run with a shown member is not muted"
        );
        assert_ne!(
            cell(&harness, 1, 2).1.fg,
            A,
            "a run of muted members is muted"
        );
        Ok(())
    }

    #[test]
    fn keys_and_the_pointer_move_by_runs_when_the_columns_fit() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 4, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(12), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_fit(true);
            Ok(())
        })?;
        let cursor = |harness: &mut Harness| {
            harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.cursor()))
        };
        chart(&mut harness, |chart, ctx| chart.cursor_by(ctx, -1))?;
        assert_eq!(
            cursor(&mut harness)?,
            Some(8),
            "the newest of the run before the last"
        );
        chart(&mut harness, |chart, ctx| chart.cursor_by(ctx, -5))?;
        assert_eq!(cursor(&mut harness)?, Some(2), "the first run");
        chart(&mut harness, |chart, ctx| chart.cursor_newest(ctx))?;
        assert_eq!(cursor(&mut harness)?, Some(11));
        mouse(&mut harness, mouse::Action::Moved, 1, 1)?;
        let hover = harness.with_unique(|chart: &mut ColumnChart, _| Ok(chart.hover()))?;
        assert_eq!(hover, Some(5), "the pointer names the newest of its run");
        mouse(&mut harness, mouse::Action::Down, 0, 1)?;
        assert_eq!(cursor(&mut harness)?, Some(2));
        harness.render()?;
        assert_eq!(screen(&harness)[3], "▲△──", "a click keeps the hover");
        Ok(())
    }

    #[test]
    fn cursor_keys_do_not_move_within_one_fitted_cell() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 1, 6)?;
        chart(&mut harness, |chart, ctx| {
            chart.set_columns(bars(4), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_fit(true);
            chart.cursor_to(ctx, 1)?;
            chart.cursor_by(ctx, -1)?;
            assert_eq!(chart.cursor(), Some(1), "all columns share one cell");
            chart.cursor_by(ctx, 1)?;
            assert_eq!(chart.cursor(), Some(1));
            Ok(())
        })?;
        assert_eq!(changes(&mut harness), 1, "only cursor_to posted a move");
        Ok(())
    }

    #[test]
    fn the_runs_follow_new_columns_and_a_new_fit_before_the_next_render() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 4, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(12), Scale::linear(8.0), Scale::linear(8.0));
            chart.set_fit(true);
            Ok(())
        })?;
        let (group, drawn) = harness.with_unique(|chart: &mut ColumnChart, _| {
            chart.set_columns(bars(5), Scale::linear(8.0), Scale::linear(8.0));
            Ok((chart.group(4), chart.drawn_upper(4)))
        })?;
        assert_eq!(group, Some(3..5), "five columns on four cells");
        assert!(drawn.is_some_and(|drawn| (3..5).contains(&drawn)));
        let group = harness.with_unique(|chart: &mut ColumnChart, _| {
            chart.set_fit(false);
            Ok(chart.group(4))
        })?;
        assert_eq!(group, Some(4..5), "one column a cell");
        Ok(())
    }

    #[test]
    fn cursor_to_moves_the_cursor_and_a_new_fit_clears_the_hover() -> Result<()> {
        let mut harness = harness(ColumnChart::new().with_scale_labels(false), 4, 6)?;
        chart(&mut harness, |chart, _| {
            chart.set_columns(bars(12), Scale::linear(8.0), Scale::linear(8.0));
            Ok(())
        })?;
        mouse(&mut harness, mouse::Action::Moved, 1, 1)?;
        let before = changes(&mut harness);
        chart(&mut harness, |chart, ctx| chart.cursor_to(ctx, 2))?;
        let (cursor, hover) = harness
            .with_unique(|chart: &mut ColumnChart, _| Ok((chart.cursor(), chart.hover())))?;
        assert_eq!((cursor, hover), (Some(2), None));
        assert_eq!(
            changes(&mut harness),
            before + 1,
            "the move posts the command"
        );
        mouse(&mut harness, mouse::Action::Moved, 1, 1)?;
        let hover = harness.with_unique(|chart: &mut ColumnChart, _| {
            chart.set_fit(true);
            Ok(chart.hover())
        })?;
        assert_eq!(hover, None);
        Ok(())
    }
}
