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

/// The grid of a cell: how many font pixels it holds, at scale 1.
///
/// On the half grid, a cell holds one pixel above another, drawn with half
/// blocks. On the octant grid, a cell holds two pixels across and four down,
/// drawn with octants. A pixel is square on a 1:2 cell in both, and an octant
/// pixel is half the size of a half-block pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum BigGrid {
    /// Two pixels in each cell, drawn with half blocks.
    #[default]
    Half,
    /// Eight pixels in each cell, drawn with octants. It needs the octants of
    /// Symbols for Legacy Computing Supplement.
    Octant,
}

impl BigGrid {
    /// Pixels across a cell at scale 1.
    pub(super) fn across(self) -> u32 {
        match self {
            Self::Half => 1,
            Self::Octant => 2,
        }
    }

    /// Pixels down a cell at scale 1.
    pub(super) fn down(self) -> u32 {
        match self {
            Self::Half => 2,
            Self::Octant => 4,
        }
    }

    /// Quarter rows of one pixel at scale 1, a unit that both grids share.
    fn quarters(self) -> u32 {
        4 / self.down()
    }
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

    /// Returns the grid rows of one line box at `scale`.
    pub(super) fn line_rows(&self, scale: u32) -> u32 {
        self.box_height().saturating_mul(scale)
    }

    /// Returns the grid rows from the top of one line to the top of the
    /// next at `scale`: the whole cell rows of a line box, and one blank row.
    pub(super) fn pitch(&self, scale: u32, grid: BigGrid) -> u32 {
        self.line_rows(scale)
            .div_ceil(grid.down())
            .saturating_add(1)
            .saturating_mul(grid.down())
    }

    /// Returns the grid rows that the text takes at `scale`. Each line
    /// starts a whole number of cell rows below the last, one blank row
    /// apart.
    pub(super) fn grid_rows(&self, scale: u32, grid: BigGrid) -> u32 {
        let lines = u32::try_from(self.lines.len()).unwrap_or(u32::MAX).max(1);
        (lines - 1)
            .saturating_mul(self.pitch(scale, grid))
            .saturating_add(self.line_rows(scale))
    }

    /// Returns the size in cells that the text takes at `scale`.
    pub(super) fn size(&self, scale: u32, grid: BigGrid) -> Size {
        Size::new(
            self.width().saturating_mul(scale).div_ceil(grid.across()),
            self.grid_rows(scale, grid).div_ceil(grid.down()),
        )
    }

    /// Returns the largest scale that fits `rows` rows and, when given,
    /// `columns` columns, or 0 when none does.
    fn max_scale(&self, rows: u32, columns: Option<u32>, grid: BigGrid) -> u32 {
        let lines = u32::try_from(self.lines.len()).unwrap_or(u32::MAX).max(1);
        let room = rows.saturating_mul(grid.down());
        // The line boxes alone bound the scale. The rows that round each
        // line up and separate the lines can take a few steps off it.
        let mut by_rows = room / lines.saturating_mul(self.box_height());
        while by_rows > 0 && self.grid_rows(by_rows, grid) > room {
            by_rows -= 1;
        }
        let by_columns = match (columns, self.width()) {
            (Some(columns), width) if width > 0 => columns.saturating_mul(grid.across()) / width,
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

/// The face, scale, grid, and layout that draw text.
#[derive(Debug, Clone)]
pub(super) struct Resolved {
    /// The text laid out in the chosen face.
    pub(super) laid: Laid,
    /// The chosen scale.
    pub(super) scale: u32,
    /// The chosen grid.
    pub(super) grid: BigGrid,
}

impl Resolved {
    /// Returns the size in cells that the text takes.
    pub(super) fn size(&self) -> Size {
        self.laid.size(self.scale, self.grid)
    }
}

/// One candidate rendering of a fit: a face, a grid, and the largest scale
/// that fits.
struct Candidate<'a> {
    /// Position of the face in [`BigFace::ALL`].
    order: usize,
    /// The text laid out in the face.
    laid: &'a Laid,
    /// The grid.
    grid: BigGrid,
    /// The scale.
    scale: u32,
    /// The size in cells.
    size: Size,
}

impl Candidate<'_> {
    /// Quarter rows of one font pixel, a scale that both grids share.
    fn pixel(&self) -> u32 {
        self.scale.saturating_mul(self.grid.quarters())
    }

    /// The cap height in quarter rows.
    fn cap(&self) -> u32 {
        self.laid.face.cap_height().saturating_mul(self.pixel())
    }
}

/// Choose the face, scale, and grid of `lines` under `size`, in an area of at
/// most `width` columns and `height` rows. `None` is unbounded. The line box
/// also holds the glyphs of `strut`. A fit chooses from `grids`, and an exact
/// size and unbounded text take the half grid.
pub(super) fn resolve(
    lines: &[Vec<(usize, char)>],
    strut: &str,
    weight: BigWeight,
    size: BigSize,
    (width, height): (Option<u32>, Option<u32>),
    grids: &[BigGrid],
) -> Resolved {
    let rows = match size {
        BigSize::Exact { face, scale } => {
            return Resolved {
                laid: Laid::new(face, weight, lines, strut),
                scale: scale.max(1),
                grid: BigGrid::Half,
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
            grid: BigGrid::Half,
        };
    };
    let laid = BigFace::ALL.map(|face| Laid::new(face, weight, lines, strut));
    let candidates = |scale: Option<u32>| -> Vec<Candidate<'_>> {
        laid.iter()
            .enumerate()
            .flat_map(|(order, laid)| grids.iter().map(move |grid| (order, laid, *grid)))
            .filter_map(|(order, laid, grid)| {
                let scale = scale.unwrap_or_else(|| laid.max_scale(rows, width, grid));
                (scale > 0).then(|| Candidate {
                    order,
                    laid,
                    grid,
                    scale,
                    size: laid.size(scale, grid),
                })
            })
            .collect()
    };
    // The largest capitals win. At equal size, the fewer rows, then the
    // smaller pixels, which keep more of the design, then the narrower
    // text, then the half grid, then the earlier face.
    let best = candidates(None).into_iter().max_by(|a, b| {
        a.cap()
            .cmp(&b.cap())
            .then(b.size.h.cmp(&a.size.h))
            .then(b.pixel().cmp(&a.pixel()))
            .then(b.size.w.cmp(&a.size.w))
            .then((b.grid as u8).cmp(&(a.grid as u8)))
            .then(b.order.cmp(&a.order))
    });
    let chosen = best.or_else(|| {
        // Nothing fits: the smallest rendering, which the area clips.
        candidates(Some(1))
            .into_iter()
            .min_by_key(|c| (c.size.h, c.size.w, c.grid as u8, c.order))
    });
    match chosen {
        Some(c) => Resolved {
            laid: c.laid.clone(),
            scale: c.scale,
            grid: c.grid,
        },
        None => Resolved {
            laid: Laid::new(BigFace::Compact, weight, lines, strut),
            scale: 1,
            grid: BigGrid::Half,
        },
    }
}
