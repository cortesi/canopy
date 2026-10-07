//! Matching each cell of a raster to a symbol and two pens.
//!
//! For a mask with `n₁` set pixels, the pens with the least squared error are
//! the means of the pixels under each part. The matcher rounds those means to
//! the `u8` pens it emits, and scores the rounded pens exactly in integers:
//! `Q − 2·p₁·S₁ + n₁·p₁² − 2·p₀·S₀ + n₀·p₀²` for each channel, where `Q` is
//! the sum of squares and `S₁`, `S₀` the sums under and outside the mask.
//!
//! Only `S₁` depends on the mask. Each run of 4 pixels has 16 subsets, and a
//! table of their sums makes `S₁` 16 lookups for any mask.
//!
//! Squared error alone weighs every wrong pixel alike, but the eye does not.
//! A pixel that takes a value its neighborhood holds, such as an edge that
//! moves a little, passes unseen. A value that nothing near it holds is a
//! mark of its own: a spur of ink that hangs off a stroke, or a notch cut
//! into one. So each pixel also costs the square of its distance outside its
//! reach, the range of each channel that the raster holds near it, and that
//! stray counts [`STRAY_WEIGHT`] times over. The cost of a symbol is its
//! squared error plus its weighted stray, and the matcher finds the least.
//! Stray is never negative, so only a symbol whose squared error is below
//! the least cost found so far can win, and the search scores the stray of
//! few symbols.

use std::array;

use thiserror::Error;

use crate::{
    geometry::{MATRIX, PIXELS},
    symbols::{Entry, SymbolSet},
};

/// A raster that does not describe whole cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RasterError {
    /// The width or height is not a multiple of the matrix size.
    #[error("raster size {width}x{height} is not a multiple of 8")]
    Size {
        /// Raster width in pixels.
        width: usize,
        /// Raster height in pixels.
        height: usize,
    },
    /// The stride is shorter than a row.
    #[error("raster stride {stride} is less than its width {width}")]
    Stride {
        /// Raster stride in pixels.
        stride: usize,
        /// Raster width in pixels.
        width: usize,
    },
    /// The slice is too short for the size and stride.
    #[error("raster needs {needed} pixels, but has {actual}")]
    Length {
        /// Pixels the size and stride need.
        needed: usize,
        /// Pixels the slice holds.
        actual: usize,
    },
}

/// A borrowed raster of matrix pixels: 8 by 8 for each cell.
#[derive(Debug, Clone, Copy)]
pub struct RasterView<'a, P> {
    /// The pixels, row by row.
    pixels: &'a [P],
    /// Width in pixels, a multiple of 8.
    width: usize,
    /// Height in pixels, a multiple of 8.
    height: usize,
    /// Distance between the starts of two rows, in pixels.
    stride: usize,
}

impl<'a, P: Copy> RasterView<'a, P> {
    /// View `pixels` as a raster of `width` by `height` pixels whose rows
    /// start `stride` pixels apart.
    pub fn new(
        pixels: &'a [P],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<Self, RasterError> {
        if !width.is_multiple_of(MATRIX) || !height.is_multiple_of(MATRIX) {
            return Err(RasterError::Size { width, height });
        }
        if stride < width {
            return Err(RasterError::Stride { stride, width });
        }
        let needed = if height == 0 {
            0
        } else {
            (height - 1) * stride + width
        };
        if pixels.len() < needed {
            return Err(RasterError::Length {
                needed,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    /// Width in cells.
    pub fn cols(&self) -> usize {
        self.width / MATRIX
    }

    /// Height in cells.
    pub fn rows(&self) -> usize {
        self.height / MATRIX
    }

    /// The 64 pixels of the cell at `(col, row)`, row by row.
    fn cell(&self, col: usize, row: usize) -> [P; PIXELS] {
        let first = row * MATRIX * self.stride + col * MATRIX;
        array::from_fn(|i| self.pixels[first + (i / MATRIX) * self.stride + i % MATRIX])
    }
}

/// The reach of each pixel of a raster: the range of each channel that the
/// raster holds within a pixel across and half a pixel down, across cell
/// edges but not past the raster's. A terminal draws a matrix pixel half as
/// wide as it is tall, so the two are about the same distance to the eye,
/// and a spur one pixel long strays. Half a pixel down holds the values
/// midway between a pixel and those above and below it. Channels lie flat,
/// `C` to a pixel.
#[derive(Debug)]
struct Reach<const C: usize> {
    /// Values across a row: the raster's width times `C`.
    row: usize,
    /// The least value of each channel.
    low: Vec<u8>,
    /// The greatest value of each channel.
    high: Vec<u8>,
}

/// Pixels of `C` channels that lie flat in memory.
trait Channels<const C: usize>: Copy {
    /// The channels of a row of pixels, `C` to a pixel.
    fn flat(row: &[Self]) -> &[u8];
}

impl Channels<3> for [u8; 3] {
    fn flat(row: &[Self]) -> &[u8] {
        row.as_flattened()
    }
}

impl Channels<1> for u8 {
    fn flat(row: &[Self]) -> &[u8] {
        row
    }
}

impl<const C: usize> Reach<C> {
    /// The reach of each pixel of a raster.
    fn of<P: Channels<C>>(raster: &RasterView<'_, P>) -> Self {
        let row = raster.width * C;
        let len = row * raster.height;
        let line = |y: usize| P::flat(&raster.pixels[y * raster.stride..][..raster.width]);
        // The extremes half a pixel down, then across.
        let (mut low, mut high) = (vec![0; row], vec![0; row]);
        let mut reach = Self {
            row,
            low: vec![0; len],
            high: vec![0; len],
        };
        for y in 0..raster.height {
            let near = [y.saturating_sub(1), y, (y + 1).min(raster.height - 1)].map(line);
            half_down(near, &mut low, &mut high);
            let out = y * row..(y + 1) * row;
            across(&low, C, &mut reach.low[out.clone()], u8::min);
            across(&high, C, &mut reach.high[out], u8::max);
        }
        reach
    }

    /// The reach of fixed pens. A pixel can show only full ink or bare
    /// ground, so ink is in reach wherever any ink is, and ground wherever
    /// any ground is: a thin or faint stroke may draw at full ink, but ink
    /// with no ink near it strays.
    fn fixed(mut self) -> Self {
        for low in &mut self.low {
            *low = if *low < u8::MAX { 0 } else { u8::MAX };
        }
        for high in &mut self.high {
            *high = if *high > 0 { u8::MAX } else { 0 };
        }
        self
    }

    /// The stray of a mask drawn with two pens in the cell at `(col, row)`:
    /// the squared distance of each pixel's pen outside its range, summed.
    /// Once the sum passes `limit`, it returns early with a sum above it.
    fn stray(
        &self,
        (col, row): (usize, usize),
        mask: u64,
        set_pen: [u8; C],
        clear_pen: [u8; C],
        limit: i64,
    ) -> i64 {
        let first = row * MATRIX * self.row + col * MATRIX * C;
        let mut total = 0;
        for y in 0..MATRIX {
            let at = first + y * self.row;
            let (low, high) = (
                &self.low[at..at + MATRIX * C],
                &self.high[at..at + MATRIX * C],
            );
            // Bit 7 of `bits` is the row's first pixel.
            let bits = (mask >> (PIXELS - MATRIX * (y + 1))) as u8;
            let mut sum = 0u32;
            for x in 0..MATRIX {
                // All ones where the mask is set, else zero.
                let set = 0u8.wrapping_sub((bits >> (MATRIX - 1 - x)) & 1);
                for ch in 0..C {
                    let pen = (set_pen[ch] & set) | (clear_pen[ch] & !set);
                    let (low, high) = (low[x * C + ch], high[x * C + ch]);
                    // At most one side is past its bound.
                    let outside = u32::from(pen.saturating_sub(high) | low.saturating_sub(pen));
                    sum += outside * outside;
                }
            }
            total += i64::from(sum);
            if total > limit {
                break;
            }
        }
        total
    }
}

/// Write into `out` the extreme by `pick` of each value of a row and of the
/// values `step` places to each side of it.
fn across(row: &[u8], step: usize, out: &mut [u8], pick: impl Fn(u8, u8) -> u8 + Copy) {
    out.copy_from_slice(row);
    if row.len() > step {
        let len = row.len();
        for (o, v) in out[step..].iter_mut().zip(&row[..len - step]) {
            *o = pick(*o, *v);
        }
        for (o, v) in out[..len - step].iter_mut().zip(&row[step..]) {
            *o = pick(*o, *v);
        }
    }
}

/// Write into `low` and `high` the extremes of each value of a row and the
/// values midway between it and the rows above and below. A midpoint
/// rounds down for `low` and up for `high`, so the range holds it exactly,
/// and inverting the raster inverts the range.
fn half_down([above, here, below]: [&[u8]; 3], low: &mut [u8], high: &mut [u8]) {
    let rows = above.iter().zip(here).zip(below);
    for ((l, h), ((a, v), b)) in low.iter_mut().zip(high.iter_mut()).zip(rows) {
        let up = |w: u8| ((u16::from(*v) + u16::from(w)).div_ceil(2)) as u8;
        *l = (*v).min(v.midpoint(*a)).min(v.midpoint(*b));
        *h = (*v).max(up(*a)).max(up(*b));
    }
}

/// A symbol with its pens and squared error, as the search scores it.
#[derive(Debug, Clone, Copy)]
struct Candidate<const C: usize> {
    /// The squared error.
    error: i64,
    /// The index of the entry in its set.
    index: usize,
    /// The pen of the mask's set pixels.
    set_pen: [u8; C],
    /// The pen of the mask's clear pixels.
    clear_pen: [u8; C],
}

/// How many times over a stray error counts, on top of its squared error. On
/// the suite of `mosaic-score`, stray falls steeply as the weight rises to
/// this, and beyond it each step costs more in difference than it saves in
/// stray.
const STRAY_WEIGHT: i64 = 2;

/// Candidates of least squared error that the search keeps for scoring
/// stray. A cell rarely has more within its bound.
const KEPT: usize = 4;

/// The squared error and position of the candidates of least squared error,
/// least first, and earliest first on a tie.
#[derive(Debug, Clone, Copy)]
struct Kept([(i64, usize); KEPT]);

impl Kept {
    /// No candidates yet.
    const EMPTY: Self = Self([(i64::MAX, usize::MAX); KEPT]);

    /// Keep the candidate at `at` if it is among the least. Candidates come
    /// in order of position.
    #[inline]
    fn offer(&mut self, error: i64, at: usize) {
        let kept = &mut self.0;
        if error < kept[KEPT - 1].0 {
            let mut i = KEPT - 1;
            while i > 0 && error < kept[i - 1].0 {
                kept[i] = kept[i - 1];
                i -= 1;
            }
            kept[i] = (error, at);
        }
    }
}

/// The candidate of least cost, squared error plus weighted stray, for the
/// cell at `cell`, and the earliest on a tie. `candidate` scores the candidate
/// at a position, `count` is the number of positions, and `kept` holds those of
/// least squared error.
fn least_cost<const C: usize>(
    kept: &Kept,
    count: usize,
    candidate: impl Fn(usize) -> Candidate<C>,
    set: &SymbolSet,
    reach: &Reach<C>,
    cell: (usize, usize),
) -> Candidate<C> {
    // The weighted stray of a candidate, or a value above `limit` once it
    // passes it.
    let stray = |c: &Candidate<C>, limit: i64| {
        let mask = set.entries()[c.index].mask.0;
        STRAY_WEIGHT * reach.stray(cell, mask, c.set_pen, c.clear_pen, limit / STRAY_WEIGHT)
    };
    let first = kept.0[0].1;
    let mut best = candidate(first);
    // The least squared error often strays not at all, and then wins.
    let first_stray = stray(&best, i64::MAX);
    if first_stray == 0 {
        return best;
    }
    // Stray is never negative, so a candidate wins only when its error
    // alone is below the least cost.
    let mut least = (best.error + first_stray, first);
    let mut consider = |at: usize, scored: Candidate<C>, least: &mut (i64, usize)| {
        let cost = (scored.error + stray(&scored, least.0 - scored.error), at);
        if cost < *least {
            *least = cost;
            best = scored;
        }
    };
    for &(error, at) in &kept.0[1..] {
        if (error, at) >= least {
            return best;
        }
        consider(at, candidate(at), &mut least);
    }
    // Every kept candidate fell within the bound, so others may too.
    let last = kept.0[KEPT - 1];
    if last < least {
        for at in 0..count {
            let scored = candidate(at);
            if (scored.error, at) > last && (scored.error, at) < least {
                consider(at, scored, &mut least);
            }
        }
    }
    best
}

/// A matched cell: a character and its two pens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell<P> {
    /// The character.
    pub ch: char,
    /// The pen of the character's ink.
    pub fg: P,
    /// The pen of the rest of the cell.
    pub bg: P,
}

/// Match each cell of an opaque sRGB raster. Pens are the rounded means of
/// the two parts of the mask of least cost. Writes `cols × rows` cells, row
/// by row, into `out`, replacing what it held.
pub fn rgb_into(raster: &RasterView<'_, [u8; 3]>, set: &SymbolSet, out: &mut Vec<Cell<[u8; 3]>>) {
    let reach = Reach::of(raster);
    each_cell(raster, out, |cell, pixels| {
        best_means(&pixels, set, &reach, cell)
    });
}

/// Match each cell of a coverage raster, where 0 is ground and 255 is full
/// ink. Pens are coverages, the rounded means of the two parts.
pub fn coverage_into(raster: &RasterView<'_, u8>, set: &SymbolSet, out: &mut Vec<Cell<u8>>) {
    let reach = Reach::of(raster);
    each_cell(raster, out, |cell, pixels| {
        let cell = best_means(&pixels.map(|p| [p]), set, &reach, cell);
        Cell {
            ch: cell.ch,
            fg: cell.fg[0],
            bg: cell.bg[0],
        }
    });
}

/// Match each cell of a coverage raster with fixed pens: every pixel shows
/// full ink or bare ground. The pens are `(255, 0)` or `(0, 255)`, so a shape
/// whose character exists only inverted still draws.
pub fn fixed_into(raster: &RasterView<'_, u8>, set: &SymbolSet, out: &mut Vec<Cell<u8>>) {
    let reach = Reach::of(raster).fixed();
    each_cell(raster, out, |cell, pixels| {
        best_fixed(&pixels, set, &reach, cell)
    });
}

/// Run `matcher` on each cell, row by row, into `out`. The matcher takes
/// the cell's column and row, and its pixels.
fn each_cell<P: Copy, C>(
    raster: &RasterView<'_, P>,
    out: &mut Vec<C>,
    mut matcher: impl FnMut((usize, usize), [P; PIXELS]) -> C,
) {
    out.clear();
    out.reserve(raster.cols() * raster.rows());
    for row in 0..raster.rows() {
        for col in 0..raster.cols() {
            out.push(matcher((col, row), raster.cell(col, row)));
        }
    }
}

/// Bits of each channel lane in a packed table entry. Sixteen entries of at
/// most 4 × 255 sum to at most 16,320, so no lane carries into the next.
const LANE: u32 = 16;

/// Sums for one cell: the totals, and the subset sums of each run of 4
/// pixels.
struct Stats<const C: usize> {
    /// Sum of each channel.
    sum: [i64; C],
    /// Sum of the squares of every channel.
    squares: i64,
    /// `table[g][m]` packs the sums of the pixels of run `g` that the
    /// nibble `m` selects. Bit 3 of `m` is the run's first pixel.
    table: [[u64; 16]; 16],
}

impl<const C: usize> Stats<C> {
    /// Gather the sums of a cell.
    fn new(pixels: &[[u8; C]; PIXELS]) -> Self {
        let mut sum = [0i64; C];
        let mut squares = 0;
        for pixel in pixels {
            for (total, value) in sum.iter_mut().zip(pixel) {
                *total += i64::from(*value);
                squares += i64::from(*value) * i64::from(*value);
            }
        }
        let mut table = [[0u64; 16]; 16];
        for (run, entries) in table.iter_mut().enumerate() {
            for m in 1..16usize {
                let low = m.trailing_zeros() as usize;
                let pixel = pixels[run * 4 + (3 - low)];
                entries[m] = entries[m & (m - 1)] + pack(pixel);
            }
        }
        Self {
            sum,
            squares,
            table,
        }
    }

    /// The sum of each channel under `mask`.
    fn under(&self, mask: u64) -> [i64; C] {
        let mut packed = 0u64;
        for (run, entries) in self.table.iter().enumerate() {
            packed += entries[((mask >> (60 - 4 * run)) & 0xF) as usize];
        }
        array::from_fn(|ch| ((packed >> (LANE * ch as u32)) & 0xFFFF) as i64)
    }
}

/// Pack the channels of a pixel into the lanes of a table entry.
fn pack<const C: usize>(pixel: [u8; C]) -> u64 {
    pixel.iter().enumerate().fold(0, |packed, (ch, value)| {
        packed | u64::from(*value) << (LANE * ch as u32)
    })
}

/// Reciprocals of 0 to 64, with 0 for 0.
const RECIPROCALS: [f32; PIXELS + 1] = {
    let mut table = [0.0; PIXELS + 1];
    let mut n = 1;
    while n <= PIXELS {
        table[n] = 1.0 / n as f32;
        n += 1;
    }
    table
};

/// `sum / n`, rounded half up, for `sum` at most 64 × 255 and `n` from 1 to
/// 64. The reciprocal is close enough that the rounding is exact.
fn round_mean(sum: i64, n: usize) -> i64 {
    (sum as f32 * RECIPROCALS[n]).round() as i64
}

/// The entry of least cost with free pens for the cell at `cell`, and its
/// cell. A uniform cell draws as a space.
fn best_means<const C: usize>(
    pixels: &[[u8; C]; PIXELS],
    set: &SymbolSet,
    reach: &Reach<C>,
    cell: (usize, usize),
) -> Cell<[u8; C]> {
    if pixels.iter().all(|p| *p == pixels[0]) {
        return Cell {
            ch: ' ',
            fg: pixels[0],
            bg: pixels[0],
        };
    }
    let stats = Stats::new(pixels);
    let score = |index: usize, entry: &Entry| {
        let n1 = entry.mask.count() as usize;
        let n0 = PIXELS - n1;
        let under = stats.under(entry.mask.0);
        let mut error = stats.squares;
        let mut set_pen = [0u8; C];
        let mut clear_pen = [0u8; C];
        for ch in 0..C {
            let s1 = under[ch];
            let s0 = stats.sum[ch] - s1;
            if n1 > 0 {
                let p = round_mean(s1, n1);
                error += n1 as i64 * p * p - 2 * p * s1;
                set_pen[ch] = p as u8;
            }
            if n0 > 0 {
                let p = round_mean(s0, n0);
                error += n0 as i64 * p * p - 2 * p * s0;
                clear_pen[ch] = p as u8;
            }
        }
        Candidate {
            error,
            index,
            set_pen,
            clear_pen,
        }
    };
    let mut kept = Kept::EMPTY;
    for (index, entry) in set.entries().iter().enumerate() {
        kept.offer(score(index, entry).error, index);
    }
    let candidate = |index: usize| score(index, &set.entries()[index]);
    let best = least_cost(&kept, set.len(), candidate, set, reach, cell);
    emit(set, best.index, best.set_pen, best.clear_pen)
}

/// The entry of least cost for the cell at `cell` when every pixel shows
/// full ink or bare ground, and its cell.
fn best_fixed(
    pixels: &[u8; PIXELS],
    set: &SymbolSet,
    reach: &Reach<1>,
    cell: (usize, usize),
) -> Cell<u8> {
    let stats = Stats::new(&pixels.map(|p| [p]));
    let full = 255 * 255;
    // The error of ink on the set pixels is `Q − 510·S₁ + 255²·n₁`, and of
    // ink on the clear pixels the same for the other part.
    // Each entry has two positions: ink on its set pixels, then on its
    // clear pixels.
    let errors = |entry: &Entry| {
        let n1 = i64::from(entry.mask.count());
        let s1 = stats.under(entry.mask.0)[0];
        let s0 = stats.sum[0] - s1;
        let on_set = stats.squares - 510 * s1 + full * n1;
        let on_clear = stats.squares - 510 * s0 + full * (PIXELS as i64 - n1);
        [on_set, on_clear]
    };
    let mut kept = Kept::EMPTY;
    for (index, entry) in set.entries().iter().enumerate() {
        let [on_set, on_clear] = errors(entry);
        kept.offer(on_set, 2 * index);
        kept.offer(on_clear, 2 * index + 1);
    }
    let candidate = |at: usize| {
        let index = at / 2;
        let ink_on_set = at.is_multiple_of(2);
        let (set_pen, clear_pen) = if ink_on_set { (255, 0) } else { (0, 255) };
        Candidate {
            error: errors(&set.entries()[index])[at % 2],
            index,
            set_pen: [set_pen],
            clear_pen: [clear_pen],
        }
    };
    let best = least_cost(&kept, 2 * set.len(), candidate, set, reach, cell);
    let cell = emit(set, best.index, best.set_pen, best.clear_pen);
    Cell {
        ch: cell.ch,
        fg: cell.fg[0],
        bg: cell.bg[0],
    }
}

/// The cell for an entry and the pens of its set and clear pixels. A part
/// with no pixels takes the other part's pen, and a cell whose pens agree
/// becomes a space.
fn emit<const C: usize>(
    set: &SymbolSet,
    index: usize,
    set_pen: [u8; C],
    clear_pen: [u8; C],
) -> Cell<[u8; C]> {
    let entry = set.entries()[index];
    let n1 = entry.mask.count() as usize;
    let set_pen = if n1 == 0 { clear_pen } else { set_pen };
    let clear_pen = if n1 == PIXELS { set_pen } else { clear_pen };
    if set_pen == clear_pen {
        return Cell {
            ch: ' ',
            fg: set_pen,
            bg: set_pen,
        };
    }
    let (fg, bg) = if entry.inverted {
        (clear_pen, set_pen)
    } else {
        (set_pen, clear_pen)
    };
    Cell {
        ch: entry.ch,
        fg,
        bg,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{
        geometry::{Mask, Shape},
        symbols::{Family, Tier},
    };

    /// Brute-force reference: the rounded pens and integer error of a mask.
    fn reference<const C: usize>(
        pixels: &[[u8; C]; PIXELS],
        mask: Mask,
    ) -> (i64, [u8; C], [u8; C]) {
        let part = |on: bool| -> Vec<[u8; C]> {
            (0..PIXELS)
                .filter(|i| mask.get(i % 8, i / 8) == on)
                .map(|i| pixels[i])
                .collect()
        };
        let mean = |values: &[[u8; C]]| -> [u8; C] {
            array::from_fn(|ch| {
                if values.is_empty() {
                    return 0;
                }
                let sum: i64 = values.iter().map(|v| i64::from(v[ch])).sum();
                let n = values.len() as i64;
                ((2 * sum + n) / (2 * n)) as u8
            })
        };
        let (set, clear) = (part(true), part(false));
        let (set_pen, clear_pen) = (mean(&set), mean(&clear));
        let error = (0..PIXELS)
            .map(|i| {
                let pen = if mask.get(i % 8, i / 8) {
                    set_pen
                } else {
                    clear_pen
                };
                (0..C)
                    .map(|ch| (i64::from(pixels[i][ch]) - i64::from(pen[ch])).pow(2))
                    .sum::<i64>()
            })
            .sum();
        (error, set_pen, clear_pen)
    }

    /// Reconstruct a cell's 64 pixels from its character and pens.
    fn reconstruct<P: Copy>(cell: Cell<P>) -> [P; PIXELS] {
        let mask = Shape::of(cell.ch).expect("supported").mask();
        array::from_fn(|i| {
            if mask.get(i % 8, i / 8) {
                cell.fg
            } else {
                cell.bg
            }
        })
    }

    /// Squared error between two pixel sets.
    fn error<const C: usize>(a: &[[u8; C]; PIXELS], b: &[[u8; C]; PIXELS]) -> i64 {
        a.iter()
            .zip(b)
            .map(|(p, q)| {
                (0..C)
                    .map(|ch| (i64::from(p[ch]) - i64::from(q[ch])).pow(2))
                    .sum::<i64>()
            })
            .sum()
    }

    /// One cell of pixels as a raster.
    fn view<P: Copy>(pixels: &[P; PIXELS]) -> RasterView<'_, P> {
        RasterView::new(pixels, 8, 8, 8).expect("raster")
    }

    /// The range of each channel within reach of each pixel of a cell.
    struct Window<const C: usize> {
        /// The least value of each channel.
        low: [[u8; C]; PIXELS],
        /// The greatest value of each channel.
        high: [[u8; C]; PIXELS],
    }

    /// Brute-force reach of a pixel of an image `width` by `height`, one
    /// channel at a time: the least and greatest of each value within a
    /// pixel across, and of the values midway between it and those above and
    /// below, rounded down for the least and up for the greatest.
    fn near(
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        value: impl Fn(usize, usize) -> u8,
    ) -> (u8, u8) {
        let (mut low, mut high) = (u8::MAX, 0);
        for u in x.saturating_sub(1)..(x + 2).min(width) {
            let here = value(u, y);
            (low, high) = (low.min(here), high.max(here));
            for v in [y.checked_sub(1), Some(y + 1).filter(|v| *v < height)]
                .into_iter()
                .flatten()
            {
                let sum = u16::from(here) + u16::from(value(u, v));
                low = low.min((sum / 2) as u8);
                high = high.max(sum.div_ceil(2) as u8);
            }
        }
        (low, high)
    }

    /// Brute-force reach of each pixel of a lone cell.
    fn lone_window<const C: usize>(pixels: &[[u8; C]; PIXELS]) -> Window<C> {
        let near = |i: usize, ch: usize| near(8, 8, i % 8, i / 8, |x, y| pixels[y * 8 + x][ch]);
        Window {
            low: array::from_fn(|i| array::from_fn(|ch| near(i, ch).0)),
            high: array::from_fn(|i| array::from_fn(|ch| near(i, ch).1)),
        }
    }

    /// The window of fixed pens: any ink or any ground within reach.
    fn fixed_window(window: &Window<1>) -> Window<1> {
        Window {
            low: window.low.map(|[v]| [if v < 255 { 0 } else { 255 }]),
            high: window.high.map(|[v]| [if v > 0 { 255 } else { 0 }]),
        }
    }

    /// Brute-force stray of a shown cell against a window.
    fn stray_of<const C: usize>(shown: &[[u8; C]; PIXELS], reach: &Window<C>) -> i64 {
        (0..PIXELS)
            .map(|i| {
                (0..C)
                    .map(|ch| {
                        let (p, low, high) = (
                            i64::from(shown[i][ch]),
                            i64::from(reach.low[i][ch]),
                            i64::from(reach.high[i][ch]),
                        );
                        (p - high).max(low - p).max(0).pow(2)
                    })
                    .sum::<i64>()
            })
            .sum()
    }

    /// The cost of showing `shown` for `pixels`: squared error plus
    /// weighted stray.
    fn cost<const C: usize>(
        pixels: &[[u8; C]; PIXELS],
        shown: &[[u8; C]; PIXELS],
        reach: &Window<C>,
    ) -> i64 {
        error(pixels, shown) + STRAY_WEIGHT * stray_of(shown, reach)
    }

    /// The least cost of any entry of a set, with its pens by brute force.
    fn least_reference_cost<const C: usize>(pixels: &[[u8; C]; PIXELS], set: &SymbolSet) -> i64 {
        let reach = lone_window(pixels);
        set.entries()
            .iter()
            .map(|entry| {
                let (_, set_pen, clear_pen) = reference(pixels, entry.mask);
                let shown = array::from_fn(|i| {
                    if entry.mask.get(i % 8, i / 8) {
                        set_pen
                    } else {
                        clear_pen
                    }
                });
                cost(pixels, &shown, &reach)
            })
            .min()
            .expect("entries")
    }

    #[test]
    fn a_rounded_stroke_end_hangs_no_ink_below_the_stroke() {
        // The bottom right of a C in Bungee, as a font banner matches it: the
        // stroke fills three rows of the middle cell and rounds off at its
        // right. Least squared error alone drew a diagonal whose tail hung
        // two rows below the stroke, in ink.
        let cell = |row: usize| -> [[u8; 8]; 3] {
            match row {
                0..7 => [
                    [255; 8],
                    [255, 255, 255, 161, 0, 0, 0, 0],
                    [0, 0, 0, 0, 0, 195, 255, 255],
                ],
                7 => [
                    [255; 8],
                    [255, 255, 255, 151, 0, 0, 0, 0],
                    [0, 0, 0, 0, 0, 185, 255, 255],
                ],
                8 => [
                    [255; 8],
                    [255, 255, 255, 76, 0, 0, 0, 0],
                    [0, 0, 0, 0, 0, 110, 255, 255],
                ],
                9 => [
                    [255; 8],
                    [255, 255, 240, 6, 0, 0, 0, 0],
                    [0, 0, 0, 0, 0, 27, 254, 255],
                ],
                10 => [
                    [255; 8],
                    [224, 139, 49, 0, 0, 0, 0, 0],
                    [0, 0, 0, 0, 0, 0, 63, 151],
                ],
                _ => [[0; 8]; 3],
            }
        };
        let pixels: Vec<u8> = (0..24)
            .flat_map(|y| cell(y).into_iter().flatten())
            .collect();
        let raster = RasterView::new(&pixels, 24, 24, 24).expect("raster");
        let mut out = Vec::new();
        coverage_into(&raster, SymbolSet::tier(Tier::Full), &mut out);
        let corner = out[4];
        let shown = reconstruct(corner);
        // The stroke ends on the third row of the cell: below the next row,
        // the cell shows bare ground.
        for (i, pen) in shown.iter().enumerate().skip(4 * 8) {
            assert!(*pen < 16, "{:?} inks pixel {i} at {pen}", corner.ch);
        }
        // Squared error alone picks the diagonal.
        let plain: [u8; PIXELS] = array::from_fn(|i| pixels[(8 + i / 8) * 24 + 8 + i % 8]);
        let set = SymbolSet::tier(Tier::Full);
        let least_error = set
            .entries()
            .iter()
            .min_by_key(|entry| reference(&plain.map(|p| [p]), entry.mask).0)
            .expect("entries");
        assert_eq!(least_error.ch, '\u{1FB59}');
    }

    #[test]
    fn a_rounded_stem_end_hangs_no_ink_below_the_stem() {
        // The bottom right of the stem of a Y in Bungee. The stem's right edge
        // runs down the middle cell, and the stem ends on its third row with a
        // rounded corner. A diagonal hung its tail a row and more below the
        // stem, and the row next to the stem was within a pixel of ink.
        let cell = |row: usize| -> [[u8; 8]; 3] {
            let stem = |middle: [u8; 8]| [[255; 8], middle, [0; 8]];
            match row {
                0..7 => stem([255, 255, 255, 255, 255, 255, 147, 0]),
                7 => stem([255, 255, 255, 255, 255, 255, 137, 0]),
                8 => stem([255, 255, 255, 255, 255, 255, 62, 0]),
                9 => stem([255, 255, 255, 255, 255, 231, 2, 0]),
                10 => stem([255, 255, 255, 220, 135, 44, 0, 0]),
                _ => [[0; 8]; 3],
            }
        };
        let pixels: Vec<u8> = (0..24)
            .flat_map(|y| cell(y).into_iter().flatten())
            .collect();
        let raster = RasterView::new(&pixels, 24, 24, 24).expect("raster");
        let mut out = Vec::new();
        coverage_into(&raster, SymbolSet::tier(Tier::Full), &mut out);
        let corner = out[4];
        let shown = reconstruct(corner);
        // Below the stem, the cell shows at most a faint shade.
        for (i, pen) in shown.iter().enumerate().skip(3 * 8) {
            assert!(*pen < 64, "{:?} inks pixel {i} at {pen}", corner.ch);
        }
        // Squared error alone picks the diagonal.
        let plain: [u8; PIXELS] = array::from_fn(|i| pixels[(8 + i / 8) * 24 + 8 + i % 8]);
        let set = SymbolSet::tier(Tier::Full);
        let least_error = set
            .entries()
            .iter()
            .min_by_key(|entry| reference(&plain.map(|p| [p]), entry.mask).0)
            .expect("entries");
        assert_eq!(least_error.ch, '\u{1FB5A}');
    }

    #[test]
    fn fixed_pens_reach_any_ink_or_ground_nearby() {
        // A faint stroke down column 3: full ink there is in reach, and so is
        // bare ground beside it. Two columns over, ink is not.
        let pixels: [u8; PIXELS] = array::from_fn(|i| if i % 8 == 3 { 90 } else { 0 });
        let reach = Reach::of(&view(&pixels)).fixed();
        let at = |x: usize| (reach.low[8 + x], reach.high[8 + x]);
        assert_eq!(at(3), (0, 255));
        assert_eq!(at(4), (0, 255));
        assert_eq!(at(6), (0, 0));
    }

    #[test]
    fn rounding_by_reciprocal_matches_integer_division() {
        for n in 1..=PIXELS {
            for sum in 0..=(n as i64 * 255) {
                let exact = (2 * sum + n as i64) / (2 * n as i64);
                assert_eq!(round_mean(sum, n), exact, "{sum} / {n}");
            }
        }
    }

    #[test]
    fn rasters_check_their_shape() {
        let pixels = [0u8; 128];
        assert!(RasterView::new(&pixels, 16, 8, 16).is_ok());
        assert_eq!(
            RasterView::new(&pixels, 12, 8, 16).unwrap_err(),
            RasterError::Size {
                width: 12,
                height: 8
            }
        );
        assert_eq!(
            RasterView::new(&pixels, 16, 8, 8).unwrap_err(),
            RasterError::Stride {
                stride: 8,
                width: 16
            }
        );
        assert_eq!(
            RasterView::new(&pixels, 16, 16, 16).unwrap_err(),
            RasterError::Length {
                needed: 256,
                actual: 128
            }
        );
    }

    #[test]
    fn the_full_white_cell_scores_without_lane_carry() {
        let pixels = [[255u8; 3]; PIXELS];
        let stats = Stats::new(&pixels);
        assert_eq!(stats.under(u64::MAX), [16_320; 3]);
    }

    #[test]
    fn a_uniform_cell_is_a_space() {
        let pixels = [[10u8, 20, 30]; PIXELS];
        let mut out = Vec::new();
        rgb_into(&view(&pixels), SymbolSet::tier(Tier::Full), &mut out);
        assert_eq!(
            out,
            [Cell {
                ch: ' ',
                fg: [10, 20, 30],
                bg: [10, 20, 30]
            }]
        );
    }

    #[test]
    fn halves_draw_the_top_in_the_foreground() {
        let pixels: [[u8; 3]; PIXELS] = array::from_fn(|i| {
            if i < 32 {
                [200, 100, 40]
            } else {
                [40, 80, 120]
            }
        });
        let mut out = Vec::new();
        rgb_into(&view(&pixels), SymbolSet::tier(Tier::Halves), &mut out);
        assert_eq!(
            out,
            [Cell {
                ch: '▀',
                fg: [200, 100, 40],
                bg: [40, 80, 120]
            }]
        );
    }

    #[test]
    fn fixed_pens_swap_to_draw_a_shape_that_only_exists_inverted() {
        // Ink on the upper seven eighths: Blocks has only the lower eighth.
        let pixels: [u8; PIXELS] = array::from_fn(|i| if i < 56 { 255 } else { 0 });
        let mut out = Vec::new();
        fixed_into(&view(&pixels), SymbolSet::tier(Tier::Blocks), &mut out);
        assert_eq!(
            out,
            [Cell {
                ch: '▁',
                fg: 0,
                bg: 255
            }]
        );
    }

    #[test]
    fn empty_coverage_has_no_pens() {
        let pixels = [0u8; PIXELS];
        let mut out = Vec::new();
        coverage_into(&view(&pixels), SymbolSet::tier(Tier::Blocks), &mut out);
        assert_eq!(
            out,
            [Cell {
                ch: ' ',
                fg: 0,
                bg: 0
            }]
        );
        fixed_into(&view(&pixels), SymbolSet::tier(Tier::Blocks), &mut out);
        assert_eq!(
            out,
            [Cell {
                ch: ' ',
                fg: 0,
                bg: 0
            }]
        );
    }

    /// Any cell of three-channel pixels.
    fn rgb_cell() -> impl Strategy<Value = [[u8; 3]; PIXELS]> {
        prop::array::uniform32(any::<[u8; 3]>()).prop_flat_map(|a| {
            prop::array::uniform32(any::<[u8; 3]>())
                .prop_map(move |b| array::from_fn(|i| if i < 32 { a[i] } else { b[i - 32] }))
        })
    }

    proptest! {
        #[test]
        fn scores_equal_the_reference(pixels in rgb_cell()) {
            let stats = Stats::new(&pixels);
            for entry in SymbolSet::tier(Tier::Full).entries() {
                let (expected, ..) = reference(&pixels, entry.mask);
                let n1 = entry.mask.count() as usize;
                let under = stats.under(entry.mask.0);
                let mut error = stats.squares;
                for ch in 0..3 {
                    let s1 = under[ch];
                    let s0 = stats.sum[ch] - s1;
                    if n1 > 0 {
                        let p = round_mean(s1, n1);
                        error += n1 as i64 * p * p - 2 * p * s1;
                    }
                    if n1 < PIXELS {
                        let p = round_mean(s0, PIXELS - n1);
                        error += (PIXELS - n1) as i64 * p * p - 2 * p * s0;
                    }
                }
                prop_assert_eq!(error, expected);
            }
        }

        #[test]
        fn exact_search_finds_the_least_cost(pixels in rgb_cell(), tier in 0usize..4) {
            let set = SymbolSet::tier(Tier::ALL[tier]);
            let mut out = Vec::new();
            rgb_into(&view(&pixels), set, &mut out);
            let got = cost(&pixels, &reconstruct(out[0]), &lone_window(&pixels));
            prop_assert_eq!(got, least_reference_cost(&pixels, set));
        }

        #[test]
        fn reach_spans_a_pixel_across_and_half_a_pixel_down(
            pixels in prop::collection::vec(any::<[u8; 3]>(), 24 * 16),
        ) {
            // The raster is 16 pixels across, in rows 24 apart.
            let raster = RasterView::new(&pixels, 16, 16, 24).expect("raster");
            let reach = Reach::of(&raster);
            for i in 0..16 * 16 {
                for ch in 0..3 {
                    let (low, high) = near(16, 16, i % 16, i / 16, |x, y| pixels[y * 24 + x][ch]);
                    prop_assert_eq!(reach.low[i * 3 + ch], low);
                    prop_assert_eq!(reach.high[i * 3 + ch], high);
                }
            }
        }

        #[test]
        fn a_larger_tier_never_costs_more(pixels in rgb_cell()) {
            let reach = lone_window(&pixels);
            let mut previous = i64::MAX;
            for tier in Tier::ALL {
                let mut out = Vec::new();
                rgb_into(&view(&pixels), SymbolSet::tier(tier), &mut out);
                let cost = cost(&pixels, &reconstruct(out[0]), &reach);
                prop_assert!(cost <= previous, "{tier:?}");
                previous = cost;
            }
        }

        #[test]
        fn painted_coverage_and_its_inverse_draw_exactly(
            index in 0usize..1000,
            a in any::<u8>(),
            b in any::<u8>(),
        ) {
            // Drawing a painted mask exactly costs nothing, whichever part
            // holds the ink, so both draw back as painted.
            let set = SymbolSet::tier(Tier::Full);
            let entry = set.entries()[index % set.len()];
            let cell: [u8; PIXELS] =
                array::from_fn(|i| if entry.mask.get(i % 8, i / 8) { a } else { b });
            for pixels in [cell, cell.map(|p| 255 - p)] {
                let mut out = Vec::new();
                coverage_into(&view(&pixels), set, &mut out);
                prop_assert_eq!(reconstruct(out[0]), pixels);
            }
        }

        #[test]
        fn fixed_pens_find_the_least_cost(pixels in prop::array::uniform32(any::<u8>())) {
            let cell: [u8; PIXELS] = array::from_fn(|i| pixels[i % 32]);
            let wide = cell.map(|p| [p]);
            let reach = fixed_window(&lone_window(&wide));
            let set = SymbolSet::from_families(&Family::ALL);
            let least = set
                .entries()
                .iter()
                .flat_map(|entry| [entry.mask, entry.mask.inverse()])
                .map(|mask| {
                    let shown = array::from_fn(|i| [if mask.get(i % 8, i / 8) { 255 } else { 0 }]);
                    cost(&wide, &shown, &reach)
                })
                .min()
                .expect("entries");
            let mut out = Vec::new();
            fixed_into(&view(&cell), &set, &mut out);
            let shown = reconstruct(out[0]).map(|p| [p]);
            prop_assert_eq!(cost(&wide, &shown, &reach), least);
        }
    }
}
