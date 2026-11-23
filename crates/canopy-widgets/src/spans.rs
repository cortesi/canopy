//! One line of text in several styles: runs at the left, runs at the right,
//! and an optional fill between them, such as a rule.

use std::iter;

use canopy::{
    NodeName, ViewContext, Widget,
    error::Result,
    geom::{Line, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
    text,
};

/// A run of text in one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text.
    pub text: String,
    /// The style path of the text.
    pub style: String,
}

impl Span {
    /// Returns a run of `text` in the style `style`.
    pub fn new(text: impl Into<String>, style: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: style.into(),
        }
    }
}

/// One line of runs: runs from the left edge, runs that end at the right
/// edge, and an optional glyph that fills the space between them.
///
/// `Spans` pushes the `spans` layer and paints each run in the style path it
/// names, and the fill in its own path, such as `rule`. The right runs give
/// way when the line is too narrow for both sides, and the left runs clip at
/// the right edge. The fill keeps one cell clear on each side.
pub struct Spans {
    /// The runs from the left edge.
    left: Vec<Span>,
    /// The runs that end at the right edge.
    right: Vec<Span>,
    /// The glyph and the style path that fill the space between the sides.
    fill: Option<(char, String)>,
}

impl Spans {
    /// Constructs a line of `left`.
    pub fn new(left: Vec<Span>) -> Self {
        Self {
            left,
            right: Vec::new(),
            fill: None,
        }
    }

    /// Puts `right` at the right edge.
    #[must_use]
    pub fn with_right(mut self, right: Vec<Span>) -> Self {
        self.right = right;
        self
    }

    /// Fills the space between the sides with `glyph` in the style `style`.
    #[must_use]
    pub fn with_fill(mut self, glyph: char, style: impl Into<String>) -> Self {
        self.fill = Some((glyph, style.into()));
        self
    }

    /// Replaces the runs from the left edge.
    pub fn set_left(&mut self, left: Vec<Span>) {
        self.left = left;
    }

    /// Replaces the runs at the right edge.
    pub fn set_right(&mut self, right: Vec<Span>) {
        self.right = right;
    }
}

/// Returns the width of `spans` in cells.
fn width(spans: &[Span]) -> u32 {
    spans.iter().fold(0, |total: u32, span| {
        total.saturating_add(text::width(&span.text))
    })
}

impl Widget for Spans {
    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).fixed_height(1)
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        let gap = u32::from(!self.left.is_empty() && !self.right.is_empty());
        let wide = width(&self.left)
            .saturating_add(gap)
            .saturating_add(width(&self.right));
        c.clamp(Size::new(wide, 1))
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.push_layer("spans");
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        render.fill("", area, ' ')?;
        let (x, y) = (area.tl.x, area.tl.y);
        let edge = x + area.w;
        let mut left_end = x;
        for span in &self.left {
            let span_width = text::width(&span.text).min(edge - left_end);
            if span_width == 0 {
                break;
            }
            render.text(&span.style, Line::new(left_end, y, span_width), &span.text)?;
            left_end += span_width;
        }
        let right_width = width(&self.right);
        let right_start = edge.saturating_sub(right_width);
        let right_fits = right_width > 0 && right_start > left_end;
        if right_fits {
            let mut at = right_start;
            for span in &self.right {
                let span_width = text::width(&span.text);
                render.text(&span.style, Line::new(at, y, span_width), &span.text)?;
                at += span_width;
            }
        }
        if let Some((glyph, style)) = &self.fill {
            let start = if self.left.is_empty() {
                x
            } else {
                left_end + 1
            };
            let end = if right_fits {
                right_start.saturating_sub(1)
            } else {
                edge
            };
            if end > start {
                let rule: String = iter::repeat_n(*glyph, (end - start) as usize).collect();
                render.text(style, Line::new(start, y, end - start), &rule)?;
            }
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("spans")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{geom::Point, style::Color, testing::harness::Harness};

    use super::*;

    /// Renders `spans` on a screen `width` cells wide and returns the harness.
    fn painted(spans: Spans, width: u32) -> Harness {
        let mut harness = Harness::builder(spans)
            .size(width, 1)
            .configure(|setup| {
                setup.widget_styles(|_palette, rules| {
                    rules
                        .fg("spans/name", Color::Blue)
                        .fg("spans/rule", Color::Red)
                        .apply();
                });
                Ok(())
            })
            .build()
            .expect("harness");
        harness.render().expect("render");
        harness
    }

    /// Returns the foreground of one cell.
    fn fg(harness: &Harness, x: u32) -> Color {
        harness.buf().get(Point { x, y: 0 }).expect("cell").style.fg
    }

    #[test]
    fn runs_take_their_styles_and_the_right_side_ends_at_the_edge() {
        let spans = Spans::new(vec![Span::new("ab", "name"), Span::new("cd", "other")])
            .with_right(vec![Span::new("xy", "other")]);
        let harness = painted(spans, 10);
        assert_eq!(harness.tbuf().lines()[0], "abcd    xy");
        assert_eq!(fg(&harness, 0), Color::Blue);
        assert_ne!(fg(&harness, 2), Color::Blue);
    }

    #[test]
    fn a_fill_keeps_a_cell_clear_on_each_side() {
        let spans = Spans::new(vec![Span::new("ab", "name")])
            .with_right(vec![Span::new("z", "name")])
            .with_fill('─', "rule");
        let harness = painted(spans, 10);
        assert_eq!(harness.tbuf().lines()[0], "ab ───── z");
        assert_eq!(fg(&harness, 3), Color::Red);
        // With no right runs, the fill runs to the edge.
        let rule = Spans::new(vec![Span::new("ab", "name")]).with_fill('─', "rule");
        assert_eq!(painted(rule, 6).tbuf().lines()[0], "ab ───");
    }

    #[test]
    fn the_right_side_gives_way_when_narrow() {
        let spans = Spans::new(vec![Span::new("abcdef", "name")])
            .with_right(vec![Span::new("xyz", "name")]);
        assert_eq!(painted(spans, 8).tbuf().lines()[0], "abcdef  ");
        let clipped = Spans::new(vec![Span::new("abcdefgh", "name")]);
        assert_eq!(painted(clipped, 4).tbuf().lines()[0], "abcd");
    }
}
