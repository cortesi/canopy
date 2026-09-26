//! Motion records and cursor painting for terminal buffers.
//!
//! A buffer holds every cell at rest. A motion record marks a grapheme whose
//! style changes over time, and emission writes its current style over the
//! rest style. Every write to a cell clears the record of the grapheme it
//! overwrites, so a later layer owns its cells.

use std::{
    mem,
    time::{Duration, Instant},
};

use super::{Cell, TermBuf};
use crate::{
    core::cursor::{CursorMotion, CursorShape},
    geom::{Point, Rect},
    style::{Attr, AttrSet, Color, Coverage, Mix, MotionClocks, Paint, ResolvedStyle},
};

/// How a moving grapheme resolves its style.
#[derive(Debug, Clone, PartialEq)]
pub enum MotionStyle {
    /// Paints resolved at a point of a rectangle, as rendering resolves them.
    Paint {
        /// Foreground paint.
        fg: Paint,
        /// Background paint.
        bg: Paint,
        /// Text attributes.
        attrs: AttrSet,
        /// Rectangle that resolves a gradient.
        rect: Rect,
        /// Point within the rectangle.
        point: Point,
        /// Glyph coverage of an antialiased cell.
        coverage: Option<Coverage>,
    },
    /// The primary cursor, which moves between its own style and the style
    /// of the cell below it.
    Cursor {
        /// Style with the cursor shown.
        on: ResolvedStyle,
        /// Style of the cell below the cursor.
        off: ResolvedStyle,
        /// Motion of the cursor.
        motion: CursorMotion,
    },
}

impl MotionStyle {
    /// Resolve the style at the clocks' time.
    fn resolve(&self, clocks: &MotionClocks) -> ResolvedStyle {
        match self {
            Self::Paint {
                fg,
                bg,
                attrs,
                rect,
                point,
                coverage,
            } => {
                let style = ResolvedStyle::new(
                    fg.resolve_in_motion(*rect, *point, clocks),
                    bg.resolve_in_motion(*rect, *point, clocks),
                    *attrs,
                );
                coverage.map_or(style, |coverage| coverage.apply(style))
            }
            Self::Cursor { on, off, motion } => {
                if !clocks.focused {
                    return mix_styles(*on, *off, UNFOCUSED_CURSOR);
                }
                if clocks.paused {
                    return *on;
                }
                let elapsed = clocks.now.saturating_duration_since(clocks.cursor);
                mix_styles(*on, *off, motion.phase(elapsed))
            }
        }
    }

    /// Return the next time the style can change after the clocks' time.
    fn next_change(&self, clocks: &MotionClocks, sample: Duration) -> Option<Instant> {
        match self {
            Self::Paint { fg, bg, .. } => [
                fg.next_change(clocks, sample),
                bg.next_change(clocks, sample),
            ]
            .into_iter()
            .flatten()
            .min(),
            Self::Cursor { motion, .. } => {
                if clocks.paused || !clocks.focused {
                    return None;
                }
                motion.next_change(clocks.cursor, clocks.now, sample)
            }
        }
    }
}

/// How far an unfocused terminal mixes the primary cursor into the cell
/// below.
const UNFOCUSED_CURSOR: f32 = 0.5;

/// Mix two styles. Attributes switch at the midpoint.
fn mix_styles(on: ResolvedStyle, off: ResolvedStyle, t: f32) -> ResolvedStyle {
    if t <= 0.0 {
        return on;
    }
    if t >= 1.0 {
        return off;
    }
    ResolvedStyle::new(
        on.fg.mix(off.fg, t, Mix::Oklab),
        on.bg.mix(off.bg, t, Mix::Oklab),
        if t < 0.5 { on.attrs } else { off.attrs },
    )
}

/// One moving grapheme.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionRecord {
    /// Number of cells in the grapheme, from its base cell.
    span: usize,
    /// How the grapheme resolves its style.
    style: MotionStyle,
}

/// The cells a cursor painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPaint {
    /// Base cell of the grapheme under the cursor.
    base: usize,
    /// Number of cells in the grapheme.
    span: usize,
    /// Style with the cursor shown.
    pub on: ResolvedStyle,
    /// Style of the cell below the cursor.
    pub off: ResolvedStyle,
}

/// Return the style of a cursor over a cell.
///
/// A block takes the cursor color as its background, and the candidate
/// foreground with the highest WCAG contrast against it. An underline takes
/// the cursor color as its foreground and underlines the grapheme.
pub fn cursor_style(below: ResolvedStyle, shape: CursorShape, color: Color) -> ResolvedStyle {
    match shape {
        CursorShape::Block => {
            let candidates = [below.bg, below.fg, Color::Black, Color::White];
            let mut fg = candidates[0];
            let mut best = color.contrast_ratio(fg);
            for candidate in &candidates[1..] {
                let ratio = color.contrast_ratio(*candidate);
                if ratio > best {
                    fg = *candidate;
                    best = ratio;
                }
            }
            ResolvedStyle::new(fg, color, below.attrs)
        }
        CursorShape::Underline => {
            ResolvedStyle::new(color, below.bg, below.attrs.with(Attr::Underline))
        }
    }
}

impl TermBuf {
    /// Record motion for every grapheme whose base cell lies in `r`.
    ///
    /// `style_at` receives the buffer point of each base cell.
    pub(crate) fn set_motion(&mut self, r: Rect, style_at: impl Fn(Point) -> MotionStyle) {
        let Some(isec) = self.rect().intersect(r) else {
            return;
        };
        for y in isec.tl.y..isec.tl.y.saturating_add(isec.h) {
            for x in isec.tl.x..isec.tl.x.saturating_add(isec.w) {
                let point = Point { x, y };
                let Some(index) = self.idx(point) else {
                    continue;
                };
                if self.cells[index].continuation {
                    continue;
                }
                let span = self.grapheme_span(index).map_or(1, |range| range.len());
                self.motion.insert(
                    index,
                    MotionRecord {
                        span,
                        style: style_at(point),
                    },
                );
            }
        }
    }

    /// Clear the motion record of the grapheme based at `index`.
    pub(super) fn clear_motion(&mut self, index: usize) {
        if !self.motion.is_empty() {
            self.motion.remove(&index);
        }
    }

    /// Return whether any grapheme moves.
    pub(crate) fn has_motion(&self) -> bool {
        !self.motion.is_empty()
    }

    /// Write the current style of every moving grapheme.
    pub(crate) fn apply_motion(&mut self, clocks: &MotionClocks) {
        let motion = mem::take(&mut self.motion);
        for (&index, record) in &motion {
            let style = record.style.resolve(clocks);
            let end = index.saturating_add(record.span).min(self.cells.len());
            for cell in &mut self.cells[index..end] {
                cell.style = style;
            }
        }
        self.motion = motion;
    }

    /// Return whether the moving graphemes at the clocks' time differ from
    /// the cells of `emitted`.
    pub(crate) fn motion_differs(&self, emitted: &Self, clocks: &MotionClocks) -> bool {
        if emitted.size != self.size {
            return true;
        }
        self.motion.iter().any(|(&index, record)| {
            emitted
                .cells
                .get(index)
                .is_none_or(|cell| cell.style != record.style.resolve(clocks))
        })
    }

    /// Return the next time any moving grapheme can change.
    pub(crate) fn next_motion_change(
        &self,
        clocks: &MotionClocks,
        sample: Duration,
    ) -> Option<Instant> {
        self.motion
            .values()
            .filter_map(|record| record.style.next_change(clocks, sample))
            .min()
    }

    /// Paint a cursor over the grapheme at a location, and return the styles
    /// with and without it. An empty cell becomes a space first.
    pub(crate) fn paint_cursor(
        &mut self,
        location: Point,
        shape: CursorShape,
        color: Color,
    ) -> Option<CursorPaint> {
        let index = self.idx(location)?;
        if self.cells[index].is_empty() {
            self.cells[index] = Cell::new(' ', self.cells[index].style);
        }
        let range = self.grapheme_span(index)?;
        let off = self.cells[range.start].style;
        let on = cursor_style(off, shape, color);
        self.clear_motion(range.start);
        let paint = CursorPaint {
            base: range.start,
            span: range.len(),
            on,
            off,
        };
        for cell in &mut self.cells[range] {
            cell.style = on;
        }
        Some(paint)
    }

    /// Make the primary cursor at a location move.
    pub(crate) fn move_cursor(&mut self, paint: CursorPaint, motion: CursorMotion) {
        if motion == CursorMotion::Steady {
            return;
        }
        self.motion.insert(
            paint.base,
            MotionRecord {
                span: paint.span,
                style: MotionStyle::Cursor {
                    on: paint.on,
                    off: paint.off,
                    motion,
                },
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::{error::Result, geom::Size, style::Animation};

    fn ground() -> ResolvedStyle {
        ResolvedStyle::new(Color::White, Color::Black, AttrSet::default())
    }

    fn moving(rect: Rect) -> MotionStyle {
        MotionStyle::Paint {
            fg: Paint::animated(Animation::blink(
                Color::Red,
                Color::Blue,
                Duration::from_millis(100),
                Duration::from_millis(100),
            )),
            bg: Paint::Solid(Color::Black),
            attrs: AttrSet::default(),
            rect,
            point: Point::ZERO,
            coverage: None,
        }
    }

    fn clocks_after(ms: u64) -> MotionClocks {
        let t0 = Instant::now();
        MotionClocks {
            now: t0 + Duration::from_millis(ms),
            ..MotionClocks::at(t0)
        }
    }

    #[test]
    fn a_later_write_owns_its_cells_and_stops_the_motion_below() -> Result<()> {
        let mut buf = TermBuf::new(Size::new(3, 1), ' ', ground())?;
        let rect = Rect::new(0, 0, 3, 1);
        buf.set_motion(rect, |_| moving(rect));
        assert_eq!(buf.motion.len(), 3);
        // A modal over the middle cell, and a restyle of the last one.
        buf.put(Point { x: 1, y: 0 }, 'm', ground())?;
        buf.restyle_grapheme(Point { x: 2, y: 0 }, ground());
        assert_eq!(buf.motion.keys().copied().collect::<Vec<_>>(), [0]);
        buf.apply_motion(&clocks_after(100));
        assert_eq!(buf.cells[0].style.fg, Color::Blue);
        assert_eq!(buf.cells[1].style, ground());
        Ok(())
    }

    #[test]
    fn a_moving_wide_grapheme_keeps_its_continuation() -> Result<()> {
        let mut buf = TermBuf::new(Size::new(3, 1), ' ', ground())?;
        buf.put_grapheme(Point::ZERO, "界", ground())?;
        let rect = Rect::new(0, 0, 3, 1);
        buf.set_motion(rect, |_| moving(rect));
        assert_eq!(
            buf.motion
                .iter()
                .map(|(&at, record)| (at, record.span))
                .collect::<Vec<_>>(),
            [(0, 2), (2, 1)]
        );
        buf.apply_motion(&clocks_after(100));
        assert_eq!(buf.cells[0].style, buf.cells[1].style);
        assert_eq!(buf.cells[1].style.fg, Color::Blue);
        buf.validate_canonical()?;
        // Writing over the continuation clears the wide grapheme's record.
        buf.put(Point { x: 1, y: 0 }, 'x', ground())?;
        assert_eq!(buf.motion.keys().copied().collect::<Vec<_>>(), [2]);
        Ok(())
    }

    #[test]
    fn a_block_takes_the_candidate_with_the_most_contrast() {
        let dark = ResolvedStyle::new(Color::Grey, Color::Black, AttrSet::default());
        let light = ResolvedStyle::new(Color::DarkGrey, Color::White, AttrSet::default());
        let yellow = Color::Rgb {
            r: 229,
            g: 192,
            b: 123,
        };
        let blue = Color::Rgb {
            r: 40,
            g: 60,
            b: 140,
        };
        // A light cursor on a dark ground keeps the dark ground as its text.
        let on = cursor_style(dark, CursorShape::Block, yellow);
        assert_eq!((on.fg, on.bg), (Color::Black, yellow));
        // A dark cursor on a dark ground takes the light text.
        let on = cursor_style(dark, CursorShape::Block, blue);
        assert_eq!((on.fg, on.bg), (Color::White, blue));
        // A dark cursor on a light ground keeps the light ground as its text.
        let on = cursor_style(light, CursorShape::Block, blue);
        assert_eq!((on.fg, on.bg), (Color::White, blue));
        // A named color works like any other: black beats the grey text.
        let on = cursor_style(light, CursorShape::Block, Color::Yellow);
        assert_eq!((on.fg, on.bg), (Color::Black, Color::Yellow));
    }

    #[test]
    fn an_underline_colors_the_grapheme_and_keeps_the_ground() {
        let on = cursor_style(ground(), CursorShape::Underline, Color::Red);
        assert_eq!(on.fg, Color::Red);
        assert_eq!(on.bg, Color::Black);
        assert!(on.attrs.underline);
    }

    #[test]
    fn an_empty_cell_becomes_a_solid_cursor() -> Result<()> {
        let mut buf = TermBuf::new(Size::new(2, 1), '\0', ground())?;
        let paint = buf
            .paint_cursor(Point { x: 1, y: 0 }, CursorShape::Block, Color::Red)
            .expect("cursor paints");
        assert_eq!(buf.cells[1].ch, ' ');
        assert_eq!(buf.cells[1].style.bg, Color::Red);
        assert_eq!(paint.off, ground());
        assert!(
            buf.paint_cursor(Point { x: 2, y: 0 }, CursorShape::Block, Color::Red)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn a_blinking_cursor_record_hides_and_an_unfocused_one_dims() -> Result<()> {
        let mut buf = TermBuf::new(Size::new(1, 1), 'a', ground())?;
        let paint = buf
            .paint_cursor(Point::ZERO, CursorShape::Block, Color::Red)
            .expect("cursor paints");
        buf.move_cursor(paint, CursorMotion::BLINK);
        let mut hidden = buf.clone();
        hidden.apply_motion(&clocks_after(600));
        assert_eq!(hidden.cells[0].style, ground());
        let mut unfocused = buf.clone();
        unfocused.apply_motion(&MotionClocks {
            focused: false,
            paused: true,
            ..clocks_after(600)
        });
        assert_eq!(
            unfocused.cells[0].style.bg,
            Color::Red.mix(Color::Black, 0.5, Mix::Oklab)
        );
        assert_eq!(
            buf.next_motion_change(&clocks_after(0), Duration::from_millis(33))
                .map(|at| at.duration_since(clocks_after(0).now) >= Duration::from_millis(599)),
            Some(true)
        );
        Ok(())
    }
}
