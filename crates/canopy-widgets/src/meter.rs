//! A one-row gauge: a stacked bar over a track, with an optional label.

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
    text,
};

use crate::chart::{self, Scale, Segment};

/// Narrowest bar that a meter asks for.
const MIN_BAR: u32 = 8;

/// A one-row gauge: a stacked bar of values over a track, drawn to an eighth
/// of a cell, and a label at its end.
///
/// The values stack from the left on a scale from zero to the meter's top,
/// which is one unless set. A value past the top fills the bar. `Meter`
/// pushes the `meter` layer and paints `fill` for a single value, `track`
/// for the rest of the bar, and `label` for its label. A gradient paint on
/// `fill` spans the whole bar, track included, so the color at the end of the
/// fill tells the level.
pub struct Meter {
    /// The stacked values, each with the style path of its color.
    segments: Vec<Segment>,
    /// Top of the scale.
    max: f64,
    /// Label at the end of the bar, when the meter has one.
    label: Option<String>,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

impl Meter {
    /// Constructs an empty meter on a scale from zero to one.
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            max: 1.0,
            label: None,
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

    /// Returns the width of the label and the space before it.
    fn label_width(&self) -> u32 {
        self.label
            .as_deref()
            .map_or(0, |label| text::width(label).saturating_add(1))
    }
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
        chart::hbar(
            render,
            line,
            &Scale::linear(self.max),
            &self.segments,
            Some("track"),
        )?;
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
}
