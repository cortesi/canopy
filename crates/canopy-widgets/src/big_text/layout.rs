//! Laying big text out in a face, and choosing the face and scale that fit
//! an area.

use canopy::geom::Size;

use super::face::{BigFace, BigWeight, Glyph};

/// How big text chooses its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BigSize {
    /// The rendering with the largest capitals that fits the area: the
    /// highest cap height at its scale. At equal size it takes the fewer
    /// rows, then the lower scale, which keeps more of the design, then the
    /// narrower text, then the earlier face of [`BigFace::ALL`].
    #[default]
    Fit,
    /// The rendering that [`BigSize::Fit`] would choose in an area of at most
    /// this many rows.
    MaxRows(u32),
    /// One face at one integer scale. A scale of 0 draws at scale 1.
    Exact {
        /// The face.
        face: BigFace,
        /// Columns and half rows of one font pixel.
        scale: u32,
    },
}

/// A glyph placed on a line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placed {
    /// Pen position of the glyph, in pixels from the start of the line.
    pub(super) pen: i32,
    /// The glyph.
    pub(super) glyph: Glyph,
    /// Index of the run that paints the glyph.
    pub(super) run: usize,
    /// The character that the glyph draws.
    pub(super) ch: char,
}

/// One line of text at scale 1.
#[derive(Debug, Clone)]
pub(super) struct Line {
    /// The glyphs of the line, in order.
    pub(super) glyphs: Vec<Placed>,
    /// Pixel column of the leftmost ink.
    pub(super) left: i32,
    /// Pixel width of the ink, from the leftmost to the rightmost.
    pub(super) width: u32,
}

/// One pixel column of a line: the run that paints it, its pixels, and the
/// glyph column that it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Column {
    /// Index of the run that paints the column.
    pub(super) run: usize,
    /// Pixels from the top of the line box: bit `n` is pixel row `n`.
    pub(super) pixels: u64,
    /// Character and glyph column of the column. A gap between glyphs has
    /// none.
    pub(super) slot: Option<(char, u32)>,
}

impl Column {
    /// Returns whether pixel row `row` from the top of the line box has ink.
    pub(super) fn ink(&self, row: u32) -> bool {
        row < u64::BITS && self.pixels >> row & 1 == 1
    }
}

/// Text laid out in one face at scale 1.
#[derive(Debug, Clone)]
pub(super) struct Laid {
    /// The face.
    pub(super) face: BigFace,
    /// The weight.
    pub(super) weight: BigWeight,
    /// The lines.
    pub(super) lines: Vec<Line>,
    /// Pixel rows of the line box above the baseline: the cap height, or
    /// more for taller glyphs.
    pub(super) above: u32,
    /// Pixel rows of the line box below the baseline, for descenders.
    pub(super) below: u32,
}

impl Laid {
    /// Lay `lines` out in `face` and `weight`. Each line is a list of
    /// characters, each with the index of its run. The line box also holds
    /// the glyphs of `strut`.
    pub(super) fn new(
        face: BigFace,
        weight: BigWeight,
        lines: &[Vec<(usize, char)>],
        strut: &str,
    ) -> Self {
        let mut above = face.cap_height() as i32;
        let mut below = 0;
        for ch in strut.chars() {
            let glyph = face.glyph(weight, ch);
            if glyph.has_ink() {
                above = above.max(i32::from(glyph.top));
                below = below.max(glyph.descent());
            }
        }
        let lines = lines
            .iter()
            .map(|line| {
                let mut pen = 0_i32;
                let mut glyphs = Vec::with_capacity(line.len());
                let mut ink: Option<(i32, i32)> = None;
                for (run, ch) in line {
                    let glyph = face.glyph(weight, *ch);
                    if glyph.has_ink() {
                        let left = pen + i32::from(glyph.x);
                        let right = left + i32::from(glyph.width);
                        ink = Some(ink.map_or((left, right), |(l, r)| (l.min(left), r.max(right))));
                        above = above.max(i32::from(glyph.top));
                        below = below.max(glyph.descent());
                    }
                    glyphs.push(Placed {
                        pen,
                        glyph,
                        run: *run,
                        ch: *ch,
                    });
                    pen = pen.saturating_add(i32::from(glyph.advance));
                }
                let (left, right) = ink.unwrap_or((0, 0));
                Line {
                    glyphs,
                    left,
                    width: (right - left).max(0) as u32,
                }
            })
            .collect();
        Self {
            face,
            weight,
            lines,
            above: above.max(0) as u32,
            below: below.max(0) as u32,
        }
    }

    /// Returns the pixel rows of one line box.
    pub(super) fn box_height(&self) -> u32 {
        (self.above + self.below).max(1)
    }

    /// Returns the pixel width of the widest line.
    pub(super) fn width(&self) -> u32 {
        self.lines.iter().map(|line| line.width).max().unwrap_or(0)
    }

    /// Returns the half rows of one line box at `scale`.
    pub(super) fn line_half_rows(&self, scale: u32) -> u32 {
        self.box_height().saturating_mul(scale)
    }

    /// Returns the half rows from the top of one line to the top of the
    /// next at `scale`: the whole rows of a line box, and one blank row.
    pub(super) fn pitch(&self, scale: u32) -> u32 {
        self.line_half_rows(scale)
            .div_ceil(2)
            .saturating_add(1)
            .saturating_mul(2)
    }

    /// Returns the half rows that the text takes at `scale`. Each line
    /// starts a whole number of rows below the last, one blank row apart.
    pub(super) fn half_rows(&self, scale: u32) -> u32 {
        let lines = u32::try_from(self.lines.len()).unwrap_or(u32::MAX).max(1);
        (lines - 1)
            .saturating_mul(self.pitch(scale))
            .saturating_add(self.line_half_rows(scale))
    }

    /// Returns the size in cells that the text takes at `scale`.
    pub(super) fn size(&self, scale: u32) -> Size {
        Size::new(
            self.width().saturating_mul(scale),
            self.half_rows(scale).div_ceil(2),
        )
    }

    /// Returns the largest scale that fits `rows` rows and, when given,
    /// `columns` columns, or 0 when none does.
    fn max_scale(&self, rows: u32, columns: Option<u32>) -> u32 {
        let lines = u32::try_from(self.lines.len()).unwrap_or(u32::MAX).max(1);
        let room = rows.saturating_mul(2);
        // The line boxes alone bound the scale. The rows that round each
        // line up and separate the lines can take a few steps off it.
        let mut by_rows = room / lines.saturating_mul(self.box_height());
        while by_rows > 0 && self.half_rows(by_rows) > room {
            by_rows -= 1;
        }
        let by_columns = match (columns, self.width()) {
            (Some(columns), width) if width > 0 => columns / width,
            _ => u32::MAX,
        };
        by_rows.min(by_columns)
    }

    /// Returns the pixel columns of `line`.
    pub(super) fn columns(&self, line: &Line) -> Vec<Column> {
        let mut columns = vec![
            Column {
                run: 0,
                pixels: 0,
                slot: None,
            };
            line.width as usize
        ];
        let mut owned = vec![false; columns.len()];
        for placed in &line.glyphs {
            let glyph = placed.glyph;
            let start = placed.pen + i32::from(glyph.x) - line.left;
            for x in 0..u32::from(glyph.width) {
                let Ok(index) = usize::try_from(start + x as i32) else {
                    continue;
                };
                let Some(column) = columns.get_mut(index) else {
                    continue;
                };
                for row in 0..glyph.rows.len() {
                    if glyph.ink(x, row) {
                        // Pixel rows count from the top of the line box.
                        let from_top = self.above as i32 - i32::from(glyph.top) + row as i32;
                        if let Ok(from_top) = u32::try_from(from_top)
                            && from_top < u64::BITS
                        {
                            column.pixels |= 1 << from_top;
                        }
                    }
                }
                column.run = placed.run;
                column.slot = Some((placed.ch, x));
                owned[index] = true;
            }
        }
        // A gap takes the run of the glyph before it, and a leading gap the
        // run of the first glyph.
        let first = line.glyphs.first().map_or(0, |placed| placed.run);
        let mut run = first;
        for (column, owned) in columns.iter_mut().zip(owned) {
            if owned {
                run = column.run;
            } else {
                column.run = run;
            }
        }
        columns
    }
}

/// The face, scale, and layout that draw text.
#[derive(Debug, Clone)]
pub(super) struct Resolved {
    /// The text laid out in the chosen face.
    pub(super) laid: Laid,
    /// The chosen scale.
    pub(super) scale: u32,
}

impl Resolved {
    /// Returns the size in cells that the text takes.
    pub(super) fn size(&self) -> Size {
        self.laid.size(self.scale)
    }
}

/// Choose the face and scale of `lines` under `size`, in an area of at most
/// `width` columns and `height` rows. `None` is unbounded. The line box also
/// holds the glyphs of `strut`.
pub(super) fn resolve(
    lines: &[Vec<(usize, char)>],
    strut: &str,
    weight: BigWeight,
    size: BigSize,
    width: Option<u32>,
    height: Option<u32>,
) -> Resolved {
    let rows = match size {
        BigSize::Exact { face, scale } => {
            return Resolved {
                laid: Laid::new(face, weight, lines, strut),
                scale: scale.max(1),
            };
        }
        BigSize::Fit => height,
        BigSize::MaxRows(rows) => Some(height.map_or(rows, |height| height.min(rows))),
    };
    // Without a bound on its height, fitted text takes the compact face at
    // scale 1, so it keeps a size in a scrolling area.
    let Some(rows) = rows else {
        return Resolved {
            laid: Laid::new(BigFace::Compact, weight, lines, strut),
            scale: 1,
        };
    };
    let laid = BigFace::ALL.map(|face| Laid::new(face, weight, lines, strut));
    let best = laid
        .iter()
        .enumerate()
        .filter_map(|(order, laid)| {
            let scale = laid.max_scale(rows, width);
            (scale > 0).then(|| (order, laid, scale, laid.size(scale)))
        })
        .max_by(|a, b| {
            let cap = |laid: &Laid, scale: u32| laid.face.cap_height().saturating_mul(scale);
            cap(a.1, a.2)
                .cmp(&cap(b.1, b.2))
                .then(b.3.h.cmp(&a.3.h))
                .then(b.2.cmp(&a.2))
                .then(b.3.w.cmp(&a.3.w))
                .then(b.0.cmp(&a.0))
        });
    let (laid, scale) = match best {
        Some((_, laid, scale, _)) => (laid.clone(), scale),
        // Nothing fits: the smallest rendering, which the area clips.
        None => laid
            .iter()
            .enumerate()
            .min_by_key(|(order, laid)| {
                let size = laid.size(1);
                (size.h, size.w, *order)
            })
            .map(|(_, laid)| (laid.clone(), 1))
            .unwrap_or_else(|| (Laid::new(BigFace::Compact, weight, lines, strut), 1)),
    };
    Resolved { laid, scale }
}
