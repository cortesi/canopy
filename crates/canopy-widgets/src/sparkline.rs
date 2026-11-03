//! A series of values in one or more rows, as bars or as a braille line.

use std::collections::VecDeque;

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Point, Rect, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
};

use crate::chart::{self, Base, Braille, Scale, Segment, Shade, Tint};

/// Values a sparkline keeps by default.
const HISTORY: usize = 1024;

/// How a sparkline draws its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// One column of eighth blocks for each value.
    Bars,
    /// A braille line through the values, two values in each cell.
    Line,
}

/// A series of values drawn small: bars of eighth blocks, or a braille line.
///
/// A value of `None` is a gap. A gap in bars draws `·` in the `gap` part, and
/// a gap in a line breaks it. The values scale from zero to the largest finite
/// value in view, or to a fixed top. A value past the top, infinity included,
/// fills its column, and a value below zero or not a number draws as zero.
/// When the view is narrower than the series, it shows the latest values, so a
/// series that grows scrolls to the left. The sparkline keeps 1,024 values
/// unless [`Sparkline::with_history`] sets another limit.
///
/// `Sparkline` pushes the `sparkline` layer and paints its bars in the `bar`
/// part and its line in the `line` part. A gradient on `bar` spans each bar's
/// height, unless [`Sparkline::with_shade`] shades the bars by value.
pub struct Sparkline {
    /// The values, oldest first.
    values: VecDeque<Option<f64>>,
    /// How the values draw.
    look: Look,
    /// Rows of the sparkline.
    rows: u32,
    /// Fixed top of the scale, when one is set.
    max: Option<f64>,
    /// Most values that the sparkline keeps.
    history: usize,
    /// How the paint of the bars colors them.
    shade: Shade,
}

impl Sparkline {
    /// Constructs an empty sparkline of bars, one row high.
    pub fn bars() -> Self {
        Self::with_look(Look::Bars)
    }

    /// Constructs an empty sparkline that draws a braille line, one row high.
    pub fn line() -> Self {
        Self::with_look(Look::Line)
    }

    /// Constructs an empty sparkline of `look`.
    fn with_look(look: Look) -> Self {
        Self {
            values: VecDeque::new(),
            look,
            rows: 1,
            max: None,
            history: HISTORY,
            shade: Shade::Span,
        }
    }

    /// Sets the height in rows. A height below one is one row.
    #[must_use]
    pub fn with_rows(mut self, rows: u32) -> Self {
        self.rows = rows.max(1);
        self
    }

    /// Fixes the top of the scale, in place of the largest value in view.
    #[must_use]
    pub fn with_max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    /// Sets the most values that the sparkline keeps, at least one. Older
    /// values leave first.
    #[must_use]
    pub fn with_history(mut self, history: usize) -> Self {
        self.history = history.max(1);
        self.trim();
        self
    }

    /// Sets how the paint of `bar` colors the bars. With [`Shade::Value`],
    /// each bar takes the color that a gradient up the bars shows at its tip,
    /// so the color tells the value finer than a row.
    #[must_use]
    pub fn with_shade(mut self, shade: Shade) -> Self {
        self.shade = shade;
        self
    }

    /// Replaces the values. Each value is a number, or `None` for a gap.
    #[must_use]
    pub fn with_values<V: Into<Option<f64>>>(
        mut self,
        values: impl IntoIterator<Item = V>,
    ) -> Self {
        self.set_values(values);
        self
    }

    /// Replaces the values. Each value is a number, or `None` for a gap.
    pub fn set_values<V: Into<Option<f64>>>(&mut self, values: impl IntoIterator<Item = V>) {
        // The history bounds the values as they arrive, so a long input never
        // grows the storage past it.
        self.values.clear();
        for value in values {
            if self.values.len() == self.history {
                self.values.pop_front();
            }
            self.values.push_back(value.into());
        }
        self.values.shrink_to(self.history);
    }

    /// Adds a value after the latest, a number or `None` for a gap.
    pub fn push(&mut self, value: impl Into<Option<f64>>) {
        self.values.push_back(value.into());
        self.trim();
    }

    /// Returns the number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the sparkline has no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Drops the oldest values past the history.
    fn trim(&mut self) {
        while self.values.len() > self.history {
            self.values.pop_front();
        }
    }

    /// Returns the values per cell of width.
    fn per_cell(&self) -> usize {
        match self.look {
            Look::Bars => 1,
            Look::Line => 2,
        }
    }

    /// Returns the latest values that `columns` cells of width show.
    fn shown(&self, columns: u32) -> impl Iterator<Item = Option<f64>> + Clone + '_ {
        let count = (columns as usize * self.per_cell()).min(self.values.len());
        self.values.range(self.values.len() - count..).copied()
    }

    /// Returns the scale of the values in view.
    fn scale(&self, shown: impl Iterator<Item = Option<f64>>) -> Scale {
        // Only finite values set the top, so one infinite value does not
        // flatten the rest.
        let top = self.max.unwrap_or_else(|| {
            shown
                .flatten()
                .filter(|value| value.is_finite())
                .fold(0.0, f64::max)
        });
        Scale::linear(top)
    }

    /// Paints the values as bars in `area`.
    fn paint_bars(&self, render: &mut Render<'_>, area: Rect) -> Result<()> {
        let shown = self.shown(area.w);
        let scale = self.scale(shown.clone());
        let bottom = area.tl.y.saturating_add(area.h - 1);
        for (offset, value) in (0_u32..).zip(shown) {
            let x = area.tl.x.saturating_add(offset);
            match value {
                Some(value) => {
                    let rect = Rect::new(x, area.tl.y, 1, area.h);
                    let bar = [Segment::new(value, "bar").with_shade(self.shade)];
                    chart::column(render, rect, Base::Bottom, &scale, &bar, Tint::None)?;
                }
                None => render.text("gap", Line::new(x, bottom, 1), "·")?,
            }
        }
        Ok(())
    }

    /// Paints the values as a braille line in `area`.
    fn paint_line(&self, render: &mut Render<'_>, area: Rect) -> Result<()> {
        let shown = self.shown(area.w);
        let scale = self.scale(shown.clone());
        let mut canvas = Braille::new(area.size());
        let bottom = canvas.dots().h.saturating_sub(1);
        let mut last = None;
        for (x, value) in (0_u32..).zip(shown) {
            let point = value.map(|value| {
                // The rise is a whole number of dots from zero to `bottom`.
                let rise = (value.max(0.0) / scale.max() * f64::from(bottom))
                    .round()
                    .min(f64::from(bottom)) as u32;
                Point {
                    x,
                    y: bottom - rise,
                }
            });
            match (last, point) {
                (Some(from), Some(to)) => canvas.line(from, to),
                (None, Some(to)) => canvas.set(to),
                _ => {}
            }
            last = point;
        }
        canvas.paint(render, area.tl, "line")
    }
}

impl Widget for Sparkline {
    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let columns = self.values.len().div_ceil(self.per_cell());
        c.clamp(Size::new(
            u32::try_from(columns).unwrap_or(u32::MAX),
            self.rows,
        ))
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("sparkline");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill("", area, ' ')?;
        match self.look {
            Look::Bars => self.paint_bars(render, area),
            Look::Line => self.paint_line(render, area),
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("sparkline")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        layout::Constraint,
        style::{Color, GradientSpec, GradientStop, Paint},
        testing::harness::Harness,
    };

    use super::*;

    /// Renders `sparkline` on a screen of `width` × `height` and returns its
    /// rows without trailing spaces.
    fn rows(sparkline: Sparkline, width: u32, height: u32) -> Vec<String> {
        let mut harness = Harness::builder(sparkline)
            .size(width, height)
            .build()
            .expect("harness");
        harness.render().expect("render");
        harness
            .tbuf()
            .lines()
            .iter()
            .map(|line| line.trim_end().to_owned())
            .collect()
    }

    #[test]
    fn bars_rise_an_eighth_at_a_time_to_the_largest_value() {
        let sparkline = Sparkline::bars().with_values([1.0, 2.0, 4.0, 8.0]);
        assert_eq!(rows(sparkline, 4, 1), ["▁▂▄█"]);
    }

    #[test]
    fn a_gap_draws_a_dot_and_a_fixed_top_scales_the_bars() {
        let sparkline = Sparkline::bars()
            .with_max(16.0)
            .with_values([Some(8.0), None, Some(16.0)]);
        assert_eq!(rows(sparkline, 3, 1), ["▄·█"]);
    }

    #[test]
    fn taller_bars_span_their_rows() {
        let sparkline = Sparkline::bars().with_rows(2).with_values([4.0, 1.0, 3.0]);
        assert_eq!(rows(sparkline, 3, 2), ["█ ▄", "█▄█"]);
    }

    #[test]
    fn values_that_are_not_finite_keep_the_scale_of_the_rest() {
        let sparkline =
            Sparkline::bars().with_values([1.0, 2.0, f64::INFINITY, f64::NAN, -3.0, 4.0]);
        assert_eq!(rows(sparkline, 6, 1), ["▂▄█  █"]);
        let line = Sparkline::line().with_values([0.0, f64::INFINITY, 3.0, f64::NAN]);
        // NaN draws as zero, so the line drops to the bottom.
        assert_eq!(rows(line, 2, 1), ["⡜⢣"]);
    }

    #[test]
    fn replaced_values_keep_to_the_history() {
        let sparkline = Sparkline::bars()
            .with_history(2)
            .with_values((0..10_000).map(f64::from));
        assert_eq!(sparkline.len(), 2);
        assert_eq!(rows(sparkline, 2, 1), ["██"]);
    }

    #[test]
    fn a_narrow_view_shows_the_latest_values() {
        let mut sparkline = Sparkline::bars().with_history(3);
        for value in [8.0, 1.0, 2.0, 4.0] {
            sparkline.push(value);
        }
        assert_eq!(sparkline.len(), 3, "the history drops the oldest");
        assert_eq!(rows(sparkline, 2, 1), ["▄█"]);
    }

    #[test]
    fn a_line_draws_two_values_a_cell_and_breaks_at_a_gap() {
        let sparkline = Sparkline::line().with_values([Some(0.0), Some(3.0), None, Some(3.0)]);
        // Dots rise from the bottom row of the cell to the top row.
        assert_eq!(rows(sparkline, 2, 1), ["⡜⠈"]);
    }

    #[test]
    fn bars_shaded_by_value_take_one_color_each() -> Result<()> {
        let low = Color::Rgb { r: 0, g: 0, b: 200 };
        let high = Color::Rgb { r: 200, g: 0, b: 0 };
        // The ramp runs from the top row down, so a span shade colors the two
        // rows of a full bar apart.
        let ramp = Paint::gradient(GradientSpec::with_stops(
            90.0,
            vec![GradientStop::new(0.0, high), GradientStop::new(1.0, low)],
        ));
        let colors = |shade: Shade| -> Result<[Color; 3]> {
            let sparkline = Sparkline::bars()
                .with_rows(2)
                .with_max(4.0)
                .with_shade(shade)
                .with_values([4.0, 2.0]);
            let ramp = ramp.clone();
            let mut harness = Harness::builder(sparkline)
                .size(2, 2)
                .configure(move |setup| {
                    setup.widget_styles(move |_, rules| {
                        rules.fg("sparkline/bar", ramp.clone()).apply();
                    });
                    Ok(())
                })
                .build()?;
            harness.render()?;
            let fg = |x, y| harness.buf().get(Point { x, y }).expect("cell").style.fg;
            Ok([fg(0, 0), fg(0, 1), fg(1, 1)])
        };
        let [top, bottom, half] = colors(Shade::Span)?;
        assert_ne!(top, bottom, "a span shade ramps up the bar");
        assert_eq!(half, bottom);
        let [top, bottom, half] = colors(Shade::Value)?;
        assert_eq!([top, bottom], [high, high], "a full bar takes the high end");
        let middle = ramp.resolve_within(Rect::new(0, 0, 1, 2), 0.5, 1.0);
        assert_eq!(half, middle, "a half bar takes the color halfway up");
        Ok(())
    }

    #[test]
    fn a_sparkline_measures_its_values() {
        let bars = Sparkline::bars().with_rows(2).with_values([1.0, 2.0, 3.0]);
        let line = Sparkline::line().with_values([1.0, 2.0, 3.0]);
        let size = |sparkline: &Sparkline| match sparkline.measure(MeasureConstraints::new(
            Constraint::Unbounded,
            Constraint::Unbounded,
        )) {
            Measurement::Fixed(size) => size,
            other => panic!("{other:?}"),
        };
        assert_eq!(size(&bars), Size::new(3, 2));
        assert_eq!(size(&line), Size::new(2, 1));
        assert!(Sparkline::bars().is_empty());
    }
}
