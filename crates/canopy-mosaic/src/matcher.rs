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

use std::array;

use thiserror::Error;

use crate::{
    geometry::{MATRIX, PIXELS},
    symbols::SymbolSet,
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
/// the two parts of the best mask. Writes `cols × rows` cells, row by row,
/// into `out`, replacing what it held.
pub fn rgb_into(raster: &RasterView<'_, [u8; 3]>, set: &SymbolSet, out: &mut Vec<Cell<[u8; 3]>>) {
    each_cell(raster, out, |pixels| best_means(&pixels, set));
}

/// Match each cell of a coverage raster, where 0 is ground and 255 is full
/// ink. Pens are coverages, the rounded means of the two parts.
pub fn coverage_into(raster: &RasterView<'_, u8>, set: &SymbolSet, out: &mut Vec<Cell<u8>>) {
    each_cell(raster, out, |pixels| {
        let cell = best_means(&pixels.map(|p| [p]), set);
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
    each_cell(raster, out, |pixels| best_fixed(&pixels, set));
}

/// Run `matcher` on each cell, row by row, into `out`.
fn each_cell<P: Copy, C>(
    raster: &RasterView<'_, P>,
    out: &mut Vec<C>,
    mut matcher: impl FnMut([P; PIXELS]) -> C,
) {
    out.clear();
    out.reserve(raster.cols() * raster.rows());
    for row in 0..raster.rows() {
        for col in 0..raster.cols() {
            out.push(matcher(raster.cell(col, row)));
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

/// The best entry by least squared error with free pens, and its cell.
fn best_means<const C: usize>(pixels: &[[u8; C]; PIXELS], set: &SymbolSet) -> Cell<[u8; C]> {
    if pixels.iter().all(|p| *p == pixels[0]) {
        return Cell {
            ch: ' ',
            fg: pixels[0],
            bg: pixels[0],
        };
    }
    let stats = Stats::new(pixels);
    let mut best = (i64::MAX, 0, [0u8; C], [0u8; C]);
    for (index, entry) in set.entries().iter().enumerate() {
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
        if error < best.0 {
            best = (error, index, set_pen, clear_pen);
        }
    }
    let (_, index, set_pen, clear_pen) = best;
    emit(set, index, set_pen, clear_pen)
}

/// The best entry when every pixel shows full ink or bare ground, and its
/// cell.
fn best_fixed(pixels: &[u8; PIXELS], set: &SymbolSet) -> Cell<u8> {
    let stats = Stats::new(&pixels.map(|p| [p]));
    let full = 255 * 255;
    // The error of ink on the set pixels is `Q − 510·S₁ + 255²·n₁`, and of
    // ink on the clear pixels the same for the other part.
    let mut best = (i64::MAX, 0, 255u8, 0u8);
    for (index, entry) in set.entries().iter().enumerate() {
        let n1 = i64::from(entry.mask.count());
        let s1 = stats.under(entry.mask.0)[0];
        let s0 = stats.sum[0] - s1;
        let on_set = stats.squares - 510 * s1 + full * n1;
        let on_clear = stats.squares - 510 * s0 + full * (PIXELS as i64 - n1);
        if on_set < best.0 {
            best = (on_set, index, 255, 0);
        }
        if on_clear < best.0 {
            best = (on_clear, index, 0, 255);
        }
    }
    let (_, index, set_pen, clear_pen) = best;
    let cell = emit(set, index, [set_pen], [clear_pen]);
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
        fn exact_search_finds_the_least_error(pixels in rgb_cell(), tier in 0usize..4) {
            let set = SymbolSet::tier(Tier::ALL[tier]);
            let least = set
                .entries()
                .iter()
                .map(|entry| reference(&pixels, entry.mask).0)
                .min()
                .expect("entries");
            let mut out = Vec::new();
            rgb_into(&view(&pixels), set, &mut out);
            prop_assert_eq!(error(&pixels, &reconstruct(out[0])), least);
        }

        #[test]
        fn a_painted_mask_round_trips(
            index in 0usize..1000,
            a in any::<[u8; 3]>(),
            b in any::<[u8; 3]>(),
            tier in 0usize..4,
        ) {
            let set = SymbolSet::tier(Tier::ALL[tier]);
            let entry = set.entries()[index % set.len()];
            let pixels: [[u8; 3]; PIXELS] =
                array::from_fn(|i| if entry.mask.get(i % 8, i / 8) { a } else { b });
            let mut out = Vec::new();
            rgb_into(&view(&pixels), set, &mut out);
            prop_assert_eq!(reconstruct(out[0]), pixels);
        }

        #[test]
        fn a_larger_tier_never_does_worse(pixels in rgb_cell()) {
            let mut previous = i64::MAX;
            for tier in Tier::ALL {
                let mut out = Vec::new();
                rgb_into(&view(&pixels), SymbolSet::tier(tier), &mut out);
                let error = error(&pixels, &reconstruct(out[0]));
                prop_assert!(error <= previous, "{tier:?}");
                previous = error;
            }
        }

        #[test]
        fn inverted_coverage_swaps_the_pens(pixels in prop::array::uniform32(any::<u8>())) {
            let cell: [u8; PIXELS] = array::from_fn(|i| pixels[i % 32]);
            let inverted = cell.map(|p| 255 - p);
            let set = SymbolSet::tier(Tier::Full);
            let (mut a, mut b) = (Vec::new(), Vec::new());
            coverage_into(&view(&cell), set, &mut a);
            coverage_into(&view(&inverted), set, &mut b);
            // Rounding half up is not symmetric, so the pens can differ by
            // one, but the errors agree.
            let shown = reconstruct(a[0]).map(|p| [p]);
            let shown_inverted = reconstruct(b[0]).map(|p| [p]);
            prop_assert_eq!(
                error(&cell.map(|p| [p]), &shown),
                error(&inverted.map(|p| [p]), &shown_inverted)
            );
        }

        #[test]
        fn fixed_pens_find_the_least_error(pixels in prop::array::uniform32(any::<u8>())) {
            let cell: [u8; PIXELS] = array::from_fn(|i| pixels[i % 32]);
            let set = SymbolSet::from_families(&Family::ALL);
            let least = set
                .entries()
                .iter()
                .flat_map(|entry| [entry.mask, entry.mask.inverse()])
                .map(|mask| {
                    (0..PIXELS)
                        .map(|i| {
                            let pen: i64 = if mask.get(i % 8, i / 8) { 255 } else { 0 };
                            (i64::from(cell[i]) - pen).pow(2)
                        })
                        .sum::<i64>()
                })
                .min()
                .expect("entries");
            let mut out = Vec::new();
            fixed_into(&view(&cell), &set, &mut out);
            let shown = reconstruct(out[0]);
            let got: i64 = (0..PIXELS)
                .map(|i| (i64::from(cell[i]) - i64::from(shown[i])).pow(2))
                .sum();
            prop_assert_eq!(got, least);
        }
    }
}
