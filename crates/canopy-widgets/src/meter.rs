//! A one-row gauge: a stacked bar over a track, with markers and an optional
//! label.

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
    text,
};

use crate::chart::{self, Marker, Scale, Segment};

/// Narrowest bar that a meter asks for.
const MIN_BAR: u32 = 8;
/// The glyph of a cell of a line bar.
const LINE: &str = "━";

/// How a meter draws its bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeterLook {
    /// Solid blocks, drawn to an eighth of a cell. A gradient paint on `fill`
    /// spans the whole bar.
    #[default]
    Blocks,
    /// A heavy line over a line track, drawn in whole cells so that the line
    /// does not break where the fill ends. Each run of one part takes the
    /// paint of its style path.
    Line,
}

/// A one-row gauge: a stacked bar of values over a track, markers at values,
/// and a label at its end.
///
/// The values stack from the left on a scale from zero to the meter's top,
/// which is one unless set. A value past the top fills the bar. `Meter`
/// pushes the `meter` layer and paints `fill` for a single value, `track`
/// for the rest of the bar, and `label` for its label. A marker shows its
/// glyph in the cell of its value, over the bar, in the style path it names,
/// such as `marker`. A gradient paint on `fill` spans the whole bar of the
/// block look, track included, so the color at the end of the fill tells the
/// level.
pub struct Meter {
    /// The stacked values, each with the style path of its color.
    segments: Vec<Segment>,
    /// Top of the scale.
    max: f64,
    /// Label at the end of the bar, when the meter has one.
    label: Option<String>,
    /// How the bar draws.
    look: MeterLook,
    /// The markers, each at a value on the scale.
    markers: Vec<(f64, Marker)>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

impl Meter {
    /// Constructs an empty meter of blocks on a scale from zero to one.
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            max: 1.0,
            label: None,
            look: MeterLook::Blocks,
            markers: Vec::new(),
        }
    }

    /// Shows one value in the `fill` part.
    #[must_use]
    pub fn with_value(mut self, value: f64) -> Self {
        self.set_value(value);
        self
    }

    /// Shows stacked values, each in the color of its style path.
    #[must_use]
    pub fn with_segments(mut self, segments: Vec<Segment>) -> Self {
        self.segments = segments;
        self
    }

    /// Sets the top of the scale. A top that is not a positive finite number
    /// is one.
    #[must_use]
    pub fn with_max(mut self, max: f64) -> Self {
        self.set_max(max);
        self
    }

    /// Shows a label at the end of the bar.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Draws the bar in `look`.
    #[must_use]
    pub fn with_look(mut self, look: MeterLook) -> Self {
        self.look = look;
        self
    }

    /// Shows `marker` at `value` on the scale, such as a limit or a target.
    #[must_use]
    pub fn with_marker(mut self, value: f64, marker: Marker) -> Self {
        self.markers.push((value, marker));
        self
    }

    /// Shows one value in the `fill` part.
    pub fn set_value(&mut self, value: f64) {
        self.segments = vec![Segment::new(value, "fill")];
    }

    /// Shows stacked values, each in the color of its style path.
    pub fn set_segments(&mut self, segments: Vec<Segment>) {
        self.segments = segments;
    }

    /// Sets the top of the scale. A top that is not a positive finite number
    /// is one.
    pub fn set_max(&mut self, max: f64) {
        self.max = Scale::linear(max).max();
    }

    /// Shows a label at the end of the bar, or no label.
    pub fn set_label(&mut self, label: Option<String>) {
        self.label = label;
    }

    /// Draws the bar in `look`.
    pub fn set_look(&mut self, look: MeterLook) {
        self.look = look;
    }

    /// Shows `markers`, each at its value on the scale, in place of the
    /// markers before.
    pub fn set_markers(&mut self, markers: Vec<(f64, Marker)>) {
        self.markers = markers;
    }

    /// Returns the width of the label and the space before it.
    fn label_width(&self) -> u32 {
        self.label
            .as_deref()
            .map_or(0, |label| text::width(label).saturating_add(1))
    }

    /// Paints the bar of the line look in `line`. Each cell belongs to the
    /// first segment that reaches past it and shows the line in its color,
    /// and the cells that no segment reaches show the track. A glyph of half
    /// a line would leave the other half of its cell empty, because a cell
    /// holds one color, so the line keeps to whole cells.
    fn paint_line(&self, render: &mut Render<'_>, line: Line) -> Result<()> {
        let ends = segment_ends(&self.segments, self.max, line.w);
        let style = |x: u32| {
            ends.iter()
                .position(|end| *end > x)
                .map_or("track", |first| self.segments[first].style.as_str())
        };
        let mut x = 0;
        while x < line.w {
            let run_style = style(x);
            let mut run = 1;
            while x + run < line.w && style(x + run) == run_style {
                run += 1;
            }
            let at = Line::new(line.tl.x + x, line.tl.y, run);
            render.text(run_style, at, &LINE.repeat(run as usize))?;
            x += run;
        }
        Ok(())
    }

    /// Paints the markers over the bar in `line`, the marker of the highest
    /// rank last.
    fn paint_markers(&self, render: &mut Render<'_>, line: Line) -> Result<()> {
        if line.w == 0 {
            return Ok(());
        }
        let mut markers: Vec<&(f64, Marker)> = self.markers.iter().collect();
        markers.sort_by_key(|(_, marker)| marker.rank);
        for (value, marker) in markers {
            let Some(cell) = marker_cell(*value, self.max, line.w) else {
                continue;
            };
            let glyph = marker.glyph.to_string();
            render.text(
                &marker.style,
                Line::new(line.tl.x + cell, line.tl.y, 1),
                &glyph,
            )?;
        }
        Ok(())
    }
}

/// Returns the end of each of `segments`, stacked on a scale from zero to
/// `max`, in cells of a bar `width` cells wide.
fn segment_ends(segments: &[Segment], max: f64, width: u32) -> Vec<u32> {
    let mut total = 0.0;
    segments
        .iter()
        .map(|segment| {
            if segment.value.is_finite() && segment.value > 0.0 {
                total += segment.value;
            }
            let share = (total / max).clamp(0.0, 1.0);
            (share * f64::from(width)).round() as u32
        })
        .collect()
}

/// Returns the cell of a bar `width` cells wide that shows a marker at
/// `value` on a scale from zero to `max`, or `None` for a value that is not a
/// number. A value off the scale marks the cell at its end.
fn marker_cell(value: f64, max: f64, width: u32) -> Option<u32> {
    if !value.is_finite() || width == 0 {
        return None;
    }
    let share = (value / max).clamp(0.0, 1.0);
    let cell = (share * f64::from(width)).round() as u32;
    Some(cell.min(width - 1))
}

impl Widget for Meter {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        c.clamp(Size::new(MIN_BAR.saturating_add(self.label_width()), 1))
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("meter");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill("", area, ' ')?;
        // The label gives way to the bar when the meter is narrow.
        let label = self
            .label
            .as_deref()
            .filter(|_| area.w > self.label_width().saturating_add(1));
        let bar = match label {
            Some(_) => area.w - self.label_width(),
            None => area.w,
        };
        let line = Line::new(area.tl.x, area.tl.y, bar);
        match self.look {
            MeterLook::Blocks => chart::hbar(
                render,
                line,
                &Scale::linear(self.max),
                &self.segments,
                Some("track"),
            )?,
            MeterLook::Line => self.paint_line(render, line)?,
        }
        self.paint_markers(render, line)?;
        if let Some(label) = label {
            let x = area.tl.x.saturating_add(bar).saturating_add(1);
            render.text("label", Line::new(x, area.tl.y, text::width(label)), label)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("meter")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::Point,
        style::{Color, ResolvedStyle},
        testing::harness::Harness,
    };

    use super::*;

    /// Fill color of the tests.
    const FILL: Color = Color::Rgb { r: 200, g: 0, b: 0 };
    /// Track color of the tests.
    const TRACK: Color = Color::Rgb {
        r: 40,
        g: 40,
        b: 40,
    };
    /// Marker color of the tests.
    const MARK: Color = Color::Rgb {
        r: 250,
        g: 250,
        b: 250,
    };

    /// Renders `meter` on a screen `width` cells wide and returns the harness.
    fn painted(meter: Meter, width: u32) -> Harness {
        let mut harness = Harness::builder(meter)
            .size(width, 1)
            .configure(|setup| {
                setup.widget_styles(|_palette, rules| {
                    rules
                        .fg("meter/fill", FILL)
                        .fg("meter/track", TRACK)
                        .fg("meter/other", Color::Blue)
                        .fg("meter/marker", MARK)
                        .apply();
                });
                Ok(())
            })
            .build()
            .expect("harness");
        harness.render().expect("render");
        harness
    }

    /// Returns the glyph and the style of one cell.
    fn cell(harness: &Harness, x: u32) -> (char, ResolvedStyle) {
        let cell = harness.buf().get(Point { x, y: 0 }).expect("cell");
        (cell.ch, cell.style)
    }

    #[test]
    fn a_value_fills_its_share_of_the_bar_over_the_track() {
        // 10 cells hold 80 eighths, and 0.45 of them is 36: 4 cells and a half.
        let harness = painted(Meter::new().with_value(0.45), 10);
        assert_eq!(harness.tbuf().lines()[0], "████▌█████");
        assert_eq!(cell(&harness, 0).1.fg, FILL);
        let (glyph, style) = cell(&harness, 4);
        assert_eq!(glyph, '▌');
        assert_eq!((style.fg, style.bg), (FILL, TRACK));
        assert_eq!(cell(&harness, 9).1.fg, TRACK);
    }

    #[test]
    fn a_label_ends_the_meter_and_gives_way_when_narrow() {
        let harness = painted(Meter::new().with_value(1.0).with_label("42%"), 10);
        assert_eq!(harness.tbuf().lines()[0], "██████ 42%");
        let harness = painted(Meter::new().with_value(1.0).with_label("42%"), 4);
        assert_eq!(harness.tbuf().lines()[0], "████", "the bar keeps the room");
    }

    #[test]
    fn segments_stack_on_a_scale_with_a_top() {
        let meter = Meter::new()
            .with_max(8.0)
            .with_segments(vec![Segment::new(2.0, "fill"), Segment::new(2.0, "other")]);
        let harness = painted(meter, 4);
        assert_eq!(cell(&harness, 0).1.fg, FILL);
        assert_eq!(cell(&harness, 1).1.fg, Color::Blue);
        assert_eq!(cell(&harness, 2).1.fg, TRACK);
    }

    #[test]
    fn an_empty_meter_draws_its_track() {
        let harness = painted(Meter::new(), 3);
        assert_eq!(cell(&harness, 0).1.fg, TRACK);
        assert_eq!(cell(&harness, 2).1.fg, TRACK);
    }

    #[test]
    fn a_line_meter_draws_an_unbroken_line_in_whole_cells() {
        // 0.45 of 10 cells is 4.5, which rounds to 5: a half cell would leave
        // a gap between the fill and the track.
        let harness = painted(Meter::new().with_look(MeterLook::Line).with_value(0.45), 10);
        assert_eq!(harness.tbuf().lines()[0], "━━━━━━━━━━");
        assert_eq!(cell(&harness, 0).1.fg, FILL);
        assert_eq!(cell(&harness, 4).1.fg, FILL);
        assert_eq!(cell(&harness, 5).1.fg, TRACK);
        let less = painted(Meter::new().with_look(MeterLook::Line).with_value(0.44), 10);
        assert_eq!(cell(&less, 3).1.fg, FILL);
        assert_eq!(cell(&less, 4).1.fg, TRACK);
        let full = painted(Meter::new().with_look(MeterLook::Line).with_value(1.0), 4);
        assert_eq!(full.tbuf().lines()[0], "━━━━");
        assert_eq!(cell(&full, 3).1.fg, FILL);
        let empty = painted(Meter::new().with_look(MeterLook::Line), 4);
        assert_eq!(cell(&empty, 0).1.fg, TRACK);
    }

    #[test]
    fn a_line_meter_stacks_segments_in_their_colors() {
        let meter = Meter::new()
            .with_look(MeterLook::Line)
            .with_max(8.0)
            .with_segments(vec![Segment::new(2.0, "fill"), Segment::new(3.0, "other")]);
        // On 8 cells, the segments end at 2 and at 5 cells.
        let harness = painted(meter, 8);
        assert_eq!(harness.tbuf().lines()[0], "━━━━━━━━");
        let colors: Vec<Color> = (0..8).map(|x| cell(&harness, x).1.fg).collect();
        assert_eq!(
            colors,
            vec![
                FILL,
                FILL,
                Color::Blue,
                Color::Blue,
                Color::Blue,
                TRACK,
                TRACK,
                TRACK
            ]
        );
    }

    #[test]
    fn markers_show_over_the_bar_at_their_values() {
        let meter = Meter::new()
            .with_look(MeterLook::Line)
            .with_value(0.3)
            .with_marker(0.5, Marker::new('│', "marker"))
            .with_marker(f64::NAN, Marker::new('x', "marker"))
            .with_marker(2.0, Marker::new('┤', "marker"));
        let harness = painted(meter, 10);
        assert_eq!(harness.tbuf().lines()[0], "━━━━━│━━━┤");
        assert_eq!(cell(&harness, 5).1.fg, MARK);
        // Over the blocks too, and the marker of the highest rank shows.
        let meter = Meter::new()
            .with_value(1.0)
            .with_marker(0.0, Marker::new('a', "marker").with_rank(2))
            .with_marker(0.0, Marker::new('b', "marker").with_rank(1));
        let harness = painted(meter, 4);
        assert_eq!(harness.tbuf().lines()[0], "a███");
    }

    #[test]
    fn marker_cells_and_segment_ends_follow_the_scale() {
        assert_eq!(marker_cell(0.5, 1.0, 10), Some(5));
        assert_eq!(marker_cell(-1.0, 1.0, 10), Some(0));
        assert_eq!(marker_cell(1.0, 1.0, 10), Some(9));
        assert_eq!(marker_cell(f64::INFINITY, 1.0, 10), None);
        assert_eq!(marker_cell(0.5, 1.0, 0), None);
        let segments = vec![
            Segment::new(0.25, "a"),
            Segment::new(-1.0, "b"),
            Segment::new(2.0, "c"),
        ];
        assert_eq!(segment_ends(&segments, 1.0, 20), vec![5, 5, 20]);
    }
}
