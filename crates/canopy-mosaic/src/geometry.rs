//! Shapes of the mosaic characters, in unit cell coordinates.
//!
//! A cell runs from 0 to 1 on each axis, with the origin at its top-left
//! corner and `y` growing down. Each supported character is a union of
//! rectangles and convex polygons. The shapes follow the geometry that the
//! Unicode names state, such as `BLOCK SEXTANT-235` or `LOWER LEFT BLOCK
//! DIAGONAL LOWER MIDDLE LEFT TO LOWER CENTRE`.

/// Width and height of the matrix of a cell, in matrix pixels.
pub const MATRIX: usize = 8;
/// Number of matrix pixels in a cell.
pub const PIXELS: usize = MATRIX * MATRIX;

/// An 8×8 mask over the matrix of a cell.
///
/// Bit 63 is the top-left pixel. Rows run from the top, and each row runs
/// from the left, so bit `63 - (y * 8 + x)` is the pixel at `(x, y)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mask(pub u64);

impl Mask {
    /// The mask with no pixels.
    pub const EMPTY: Self = Self(0);
    /// The mask with every pixel.
    pub const FULL: Self = Self(u64::MAX);

    /// The bit of the pixel at `(x, y)`.
    fn bit(x: usize, y: usize) -> u64 {
        1 << (PIXELS - 1 - (y * MATRIX + x))
    }

    /// Build a mask from a predicate on matrix pixels.
    pub fn from_fn(mut f: impl FnMut(usize, usize) -> bool) -> Self {
        let mut bits = 0;
        for y in 0..MATRIX {
            for x in 0..MATRIX {
                if f(x, y) {
                    bits |= Self::bit(x, y);
                }
            }
        }
        Self(bits)
    }

    /// Whether the pixel at `(x, y)` is set.
    pub fn get(self, x: usize, y: usize) -> bool {
        self.0 & Self::bit(x, y) != 0
    }

    /// The number of set pixels.
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// The mask with every pixel flipped.
    #[must_use]
    pub fn inverse(self) -> Self {
        Self(!self.0)
    }
}

/// A point in unit cell coordinates.
pub type Point = [f32; 2];

/// One convex piece of a shape.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    /// An axis-aligned rectangle from `(x0, y0)` to `(x1, y1)`.
    Rect {
        /// Left edge.
        x0: f32,
        /// Top edge.
        y0: f32,
        /// Right edge.
        x1: f32,
        /// Bottom edge.
        y1: f32,
    },
    /// A convex polygon, its vertices in order.
    Polygon(Vec<Point>),
}

impl Part {
    /// The share of the rectangle `[x0, x1) × [y0, y1)` that this part
    /// covers, from 0 to 1.
    pub fn coverage_of(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
        let area = (x1 - x0) * (y1 - y0);
        if area <= 0.0 {
            return 0.0;
        }
        (self.area_in(x0, y0, x1, y1) / area).clamp(0.0, 1.0)
    }

    /// The area of this part inside the rectangle `[x0, x1) × [y0, y1)`.
    fn area_in(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
        match self {
            Self::Rect {
                x0: left,
                y0: top,
                x1: right,
                y1: bottom,
            } => {
                let w = right.min(x1) - left.max(x0);
                let h = bottom.min(y1) - top.max(y0);
                if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
            }
            Self::Polygon(points) => {
                let clipped = clip_to_rect(points, x0, y0, x1, y1);
                polygon_area(&clipped)
            }
        }
    }
}

/// A character's shape: a union of parts that do not overlap.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// The parts of the shape.
    parts: Vec<Part>,
}

impl Shape {
    /// The shape of `ch`, if it is a supported mosaic character.
    pub fn of(ch: char) -> Option<Self> {
        shape_of(ch).map(|parts| Self { parts })
    }

    /// The parts of the shape.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The share of the rectangle `[x0, x1) × [y0, y1)` that the shape
    /// covers, from 0 to 1.
    pub fn coverage_of(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
        let area = (x1 - x0) * (y1 - y0);
        if area <= 0.0 {
            return 0.0;
        }
        let covered: f32 = self
            .parts
            .iter()
            .map(|part| part.area_in(x0, y0, x1, y1))
            .sum();
        (covered / area).clamp(0.0, 1.0)
    }

    /// The 8×8 mask of the shape. A matrix pixel is set when the shape covers
    /// at least half of it.
    pub fn mask(&self) -> Mask {
        let step = 1.0 / MATRIX as f32;
        Mask::from_fn(|x, y| {
            let (x0, y0) = (x as f32 * step, y as f32 * step);
            self.coverage_of(x0, y0, x0 + step, y0 + step) >= 0.5
        })
    }

    /// Write the area coverage of the shape on a `width` by `height` pixel
    /// grid into `out`, row by row, from 0 for empty to 255 for full.
    ///
    /// # Panics
    ///
    /// Panics when `out` is shorter than `width * height`.
    pub fn coverage_into(&self, width: usize, height: usize, out: &mut [u8]) {
        assert!(out.len() >= width * height, "coverage buffer is too short");
        let (sx, sy) = (1.0 / width as f32, 1.0 / height as f32);
        for y in 0..height {
            for x in 0..width {
                let (x0, y0) = (x as f32 * sx, y as f32 * sy);
                let coverage = self.coverage_of(x0, y0, x0 + sx, y0 + sy);
                out[y * width + x] = (coverage * 255.0).round() as u8;
            }
        }
    }
}

/// Clip a convex polygon to a rectangle, edge by edge.
fn clip_to_rect(points: &[Point], x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Point> {
    let mut poly = points.to_vec();
    // Each edge keeps the points where `f` is at least zero.
    let edges: [&dyn Fn(Point) -> f32; 4] =
        [&|p| p[0] - x0, &|p| x1 - p[0], &|p| p[1] - y0, &|p| {
            y1 - p[1]
        }];
    for edge in edges {
        poly = clip_half(&poly, edge);
        if poly.is_empty() {
            break;
        }
    }
    poly
}

/// Keep the part of a convex polygon where `f(p) >= 0`, for an affine `f`.
fn clip_half(poly: &[Point], f: &dyn Fn(Point) -> f32) -> Vec<Point> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// The area of a simple polygon, by the shoelace formula.
fn polygon_area(poly: &[Point]) -> f32 {
    if poly.len() < 3 {
        return 0.0;
    }
    let twice: f32 = poly
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let b = poly[(i + 1) % poly.len()];
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    twice.abs() / 2.0
}

/// A rectangle part.
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Part {
    Part::Rect { x0, y0, x1, y1 }
}

/// The cells of a 2-column lattice with `rows` rows, from the set bits of
/// `bits`. Bit `n` is cell `n + 1` in Unicode's numbering, which runs left to
/// right and then down.
fn lattice(bits: u32, rows: u32) -> Vec<Part> {
    (0..2 * rows)
        .filter(|n| bits & (1 << n) != 0)
        .map(|n| {
            let (x, y) = ((n % 2) as f32 / 2.0, (n / 2) as f32 / rows as f32);
            rect(x, y, x + 0.5, y + 1.0 / rows as f32)
        })
        .collect()
}

/// The octant patterns that Unicode assigns to characters outside the octant
/// block, with those characters. Bit `n` is octant `n + 1`.
pub(crate) const OCTANT_ELSEWHERE: [(u8, char); 26] = [
    (0x00, ' '),
    (0x01, '\u{1CEA8}'),
    (0x02, '\u{1CEAB}'),
    (0x03, '\u{1FB82}'),
    (0x05, '\u{2598}'),
    (0x0A, '\u{259D}'),
    (0x0F, '\u{2580}'),
    (0x14, '\u{1FBE6}'),
    (0x28, '\u{1FBE7}'),
    (0x3F, '\u{1FB85}'),
    (0x40, '\u{1CEA3}'),
    (0x50, '\u{2596}'),
    (0x55, '\u{258C}'),
    (0x5A, '\u{259E}'),
    (0x5F, '\u{259B}'),
    (0x80, '\u{1CEA0}'),
    (0xA0, '\u{2597}'),
    (0xA5, '\u{259A}'),
    (0xAA, '\u{2590}'),
    (0xAF, '\u{259C}'),
    (0xC0, '\u{2582}'),
    (0xF0, '\u{2584}'),
    (0xF5, '\u{2599}'),
    (0xFA, '\u{259F}'),
    (0xFC, '\u{2586}'),
    (0xFF, '\u{2588}'),
];

/// First code point of the octant block.
pub(crate) const OCTANT_FIRST: u32 = 0x1CD00;
/// Last code point of the octant block.
pub(crate) const OCTANT_LAST: u32 = 0x1CDE5;
/// First code point of the sextant block.
pub(crate) const SEXTANT_FIRST: u32 = 0x1FB00;
/// Last code point of the sextant block.
pub(crate) const SEXTANT_LAST: u32 = 0x1FB3B;

/// The octant pattern of a character in the octant block. The block assigns
/// the patterns in order, skipping those that [`OCTANT_ELSEWHERE`] holds.
pub(crate) fn octant_bits(code: u32) -> Option<u8> {
    if !(OCTANT_FIRST..=OCTANT_LAST).contains(&code) {
        return None;
    }
    let mut index = code - OCTANT_FIRST;
    for bits in 0..=255u8 {
        if OCTANT_ELSEWHERE.iter().any(|(pattern, _)| *pattern == bits) {
            continue;
        }
        if index == 0 {
            return Some(bits);
        }
        index -= 1;
    }
    None
}

/// The sextant pattern of a character in the sextant block. The block assigns
/// the patterns 1 to 62 in order, skipping the left and right halves.
pub(crate) fn sextant_bits(code: u32) -> Option<u32> {
    if !(SEXTANT_FIRST..=SEXTANT_LAST).contains(&code) {
        return None;
    }
    let mut bits = code - SEXTANT_FIRST + 1;
    if bits >= 21 {
        bits += 1;
    }
    if bits >= 42 {
        bits += 1;
    }
    Some(bits)
}

/// A named point on the edge of a cell, as the smooth mosaics name them.
#[derive(Clone, Copy)]
enum Edge {
    /// The top-left corner.
    UpperLeft,
    /// The middle of the top edge.
    UpperCentre,
    /// The top-right corner.
    UpperRight,
    /// A third of the way down the left edge.
    UpperMiddleLeft,
    /// A third of the way down the right edge.
    UpperMiddleRight,
    /// Two thirds of the way down the left edge.
    LowerMiddleLeft,
    /// Two thirds of the way down the right edge.
    LowerMiddleRight,
    /// The bottom-left corner.
    LowerLeft,
    /// The middle of the bottom edge.
    LowerCentre,
    /// The bottom-right corner.
    LowerRight,
}

impl Edge {
    /// The point in unit cell coordinates.
    fn point(self) -> Point {
        match self {
            Self::UpperLeft => [0.0, 0.0],
            Self::UpperCentre => [0.5, 0.0],
            Self::UpperRight => [1.0, 0.0],
            Self::UpperMiddleLeft => [0.0, 1.0 / 3.0],
            Self::UpperMiddleRight => [1.0, 1.0 / 3.0],
            Self::LowerMiddleLeft => [0.0, 2.0 / 3.0],
            Self::LowerMiddleRight => [1.0, 2.0 / 3.0],
            Self::LowerLeft => [0.0, 1.0],
            Self::LowerCentre => [0.5, 1.0],
            Self::LowerRight => [1.0, 1.0],
        }
    }
}

/// The smooth mosaics: each fills the side of a line that holds a corner.
/// The entries are the code point, the corner, and the two ends of the line.
const DIAGONALS: [(u32, Edge, Edge, Edge); 44] = {
    use Edge::{
        LowerCentre, LowerLeft, LowerMiddleLeft, LowerMiddleRight, LowerRight, UpperCentre,
        UpperLeft, UpperMiddleLeft, UpperMiddleRight, UpperRight,
    };
    [
        (0x1FB3C, LowerLeft, LowerMiddleLeft, LowerCentre),
        (0x1FB3D, LowerLeft, LowerMiddleLeft, LowerRight),
        (0x1FB3E, LowerLeft, UpperMiddleLeft, LowerCentre),
        (0x1FB3F, LowerLeft, UpperMiddleLeft, LowerRight),
        (0x1FB40, LowerLeft, UpperLeft, LowerCentre),
        (0x1FB41, LowerRight, UpperMiddleLeft, UpperCentre),
        (0x1FB42, LowerRight, UpperMiddleLeft, UpperRight),
        (0x1FB43, LowerRight, LowerMiddleLeft, UpperCentre),
        (0x1FB44, LowerRight, LowerMiddleLeft, UpperRight),
        (0x1FB45, LowerRight, LowerLeft, UpperCentre),
        (0x1FB46, LowerRight, LowerMiddleLeft, UpperMiddleRight),
        (0x1FB47, LowerRight, LowerCentre, LowerMiddleRight),
        (0x1FB48, LowerRight, LowerLeft, LowerMiddleRight),
        (0x1FB49, LowerRight, LowerCentre, UpperMiddleRight),
        (0x1FB4A, LowerRight, LowerLeft, UpperMiddleRight),
        (0x1FB4B, LowerRight, LowerCentre, UpperRight),
        (0x1FB4C, LowerLeft, UpperCentre, UpperMiddleRight),
        (0x1FB4D, LowerLeft, UpperLeft, UpperMiddleRight),
        (0x1FB4E, LowerLeft, UpperCentre, LowerMiddleRight),
        (0x1FB4F, LowerLeft, UpperLeft, LowerMiddleRight),
        (0x1FB50, LowerLeft, UpperCentre, LowerRight),
        (0x1FB51, LowerLeft, UpperMiddleLeft, LowerMiddleRight),
        (0x1FB52, UpperRight, LowerMiddleLeft, LowerCentre),
        (0x1FB53, UpperRight, LowerMiddleLeft, LowerRight),
        (0x1FB54, UpperRight, UpperMiddleLeft, LowerCentre),
        (0x1FB55, UpperRight, UpperMiddleLeft, LowerRight),
        (0x1FB56, UpperRight, UpperLeft, LowerCentre),
        (0x1FB57, UpperLeft, UpperMiddleLeft, UpperCentre),
        (0x1FB58, UpperLeft, UpperMiddleLeft, UpperRight),
        (0x1FB59, UpperLeft, LowerMiddleLeft, UpperCentre),
        (0x1FB5A, UpperLeft, LowerMiddleLeft, UpperRight),
        (0x1FB5B, UpperLeft, LowerLeft, UpperCentre),
        (0x1FB5C, UpperLeft, LowerMiddleLeft, UpperMiddleRight),
        (0x1FB5D, UpperLeft, LowerCentre, LowerMiddleRight),
        (0x1FB5E, UpperLeft, LowerLeft, LowerMiddleRight),
        (0x1FB5F, UpperLeft, LowerCentre, UpperMiddleRight),
        (0x1FB60, UpperLeft, LowerLeft, UpperMiddleRight),
        (0x1FB61, UpperLeft, LowerCentre, UpperRight),
        (0x1FB62, UpperRight, UpperCentre, UpperMiddleRight),
        (0x1FB63, UpperRight, UpperLeft, UpperMiddleRight),
        (0x1FB64, UpperRight, UpperCentre, LowerMiddleRight),
        (0x1FB65, UpperRight, UpperLeft, LowerMiddleRight),
        (0x1FB66, UpperRight, UpperCentre, LowerRight),
        (0x1FB67, UpperRight, UpperMiddleLeft, LowerMiddleRight),
    ]
};

/// The side of the line from `a` to `b` that holds `corner`, within the
/// cell.
fn half_plane(corner: Edge, a: Edge, b: Edge) -> Part {
    let (a, b, corner) = (a.point(), b.point(), corner.point());
    let side = |p: Point| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let sign = side(corner).signum();
    let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    Part::Polygon(clip_half(&square, &|p| sign * side(p)))
}

/// The four triangles that meet at the center of a cell.
fn triangle(side: usize) -> Part {
    let center = [0.5, 0.5];
    let points = match side {
        0 => [[0.0, 0.0], center, [0.0, 1.0]],
        1 => [[0.0, 0.0], [1.0, 0.0], center],
        2 => [[1.0, 0.0], [1.0, 1.0], center],
        _ => [[0.0, 1.0], center, [1.0, 1.0]],
    };
    Part::Polygon(points.to_vec())
}

/// Index of the left triangle in [`triangle`].
const LEFT: usize = 0;
/// Index of the upper triangle in [`triangle`].
const UPPER: usize = 1;
/// Index of the right triangle in [`triangle`].
const RIGHT: usize = 2;
/// Index of the lower triangle in [`triangle`].
const LOWER: usize = 3;

/// A horizontal band of eighths, from row `from` up to row `to` of 8.
fn rows(from: u32, to: u32) -> Part {
    rect(0.0, from as f32 / 8.0, 1.0, to as f32 / 8.0)
}

/// A vertical band of eighths, from column `from` up to column `to` of 8.
fn columns(from: u32, to: u32) -> Part {
    rect(from as f32 / 8.0, 0.0, to as f32 / 8.0, 1.0)
}

/// The parts of a supported character.
fn shape_of(ch: char) -> Option<Vec<Part>> {
    let code = u32::from(ch);
    if let Some(bits) = octant_bits(code) {
        return Some(lattice(u32::from(bits), 4));
    }
    if let Some(bits) = sextant_bits(code) {
        return Some(lattice(bits, 3));
    }
    if let Some((_, corner, a, b)) = DIAGONALS.iter().find(|(c, ..)| *c == code) {
        return Some(vec![half_plane(*corner, *a, *b)]);
    }
    let parts = match code {
        0x20 => Vec::new(),
        // Block elements.
        0x2580 => vec![rows(0, 4)],
        0x2581..=0x2588 => vec![rows(8 - (code - 0x2580), 8)],
        0x2589..=0x258F => vec![columns(0, 8 - (code - 0x2588))],
        0x2590 => vec![columns(4, 8)],
        0x2594 => vec![rows(0, 1)],
        0x2595 => vec![columns(7, 8)],
        0x2596..=0x259F => {
            // Quadrants, as octant patterns.
            let bits = match code {
                0x2596 => 0x50,
                0x2597 => 0xA0,
                0x2598 => 0x05,
                0x2599 => 0xF5,
                0x259A => 0xA5,
                0x259B => 0x5F,
                0x259C => 0xAF,
                0x259D => 0x0A,
                0x259E => 0x5A,
                _ => 0xFA,
            };
            lattice(bits, 4)
        }
        // Triangles that meet at the center.
        0x1FB68 => vec![triangle(UPPER), triangle(RIGHT), triangle(LOWER)],
        0x1FB69 => vec![triangle(LEFT), triangle(LOWER), triangle(RIGHT)],
        0x1FB6A => vec![triangle(UPPER), triangle(LEFT), triangle(LOWER)],
        0x1FB6B => vec![triangle(LEFT), triangle(UPPER), triangle(RIGHT)],
        0x1FB6C => vec![triangle(LEFT)],
        0x1FB6D => vec![triangle(UPPER)],
        0x1FB6E => vec![triangle(RIGHT)],
        0x1FB6F => vec![triangle(LOWER)],
        // One-eighth bars, numbered from 1 at the left or the top.
        0x1FB70..=0x1FB75 => {
            let n = code - 0x1FB70 + 1;
            vec![columns(n, n + 1)]
        }
        0x1FB76..=0x1FB7B => {
            let n = code - 0x1FB76 + 1;
            vec![rows(n, n + 1)]
        }
        0x1FB7C => vec![columns(0, 1), rect(0.125, 0.875, 1.0, 1.0)],
        0x1FB7D => vec![columns(0, 1), rect(0.125, 0.0, 1.0, 0.125)],
        0x1FB7E => vec![columns(7, 8), rect(0.0, 0.0, 0.875, 0.125)],
        0x1FB7F => vec![columns(7, 8), rect(0.0, 0.875, 0.875, 1.0)],
        0x1FB80 => vec![rows(0, 1), rows(7, 8)],
        0x1FB81 => vec![rows(0, 1), rows(2, 3), rows(4, 5), rows(7, 8)],
        0x1FB82 => vec![rows(0, 2)],
        0x1FB83 => vec![rows(0, 3)],
        0x1FB84 => vec![rows(0, 5)],
        0x1FB85 => vec![rows(0, 6)],
        0x1FB86 => vec![rows(0, 7)],
        0x1FB87 => vec![columns(6, 8)],
        0x1FB88 => vec![columns(5, 8)],
        0x1FB89 => vec![columns(3, 8)],
        0x1FB8A => vec![columns(2, 8)],
        0x1FB8B => vec![columns(1, 8)],
        // Quarter blocks that complete the octant patterns.
        0x1FBE6 => lattice(0x14, 4),
        0x1FBE7 => lattice(0x28, 4),
        0x1CEA0 => lattice(0x80, 4),
        0x1CEA3 => lattice(0x40, 4),
        0x1CEA8 => lattice(0x01, 4),
        0x1CEAB => lattice(0x02, 4),
        _ => return None,
    };
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mask of a character.
    fn mask(ch: char) -> Mask {
        Shape::of(ch).expect("supported").mask()
    }

    #[test]
    fn block_elements_cover_their_eighths() {
        assert_eq!(mask(' '), Mask::EMPTY);
        assert_eq!(mask('█'), Mask::FULL);
        assert_eq!(mask('▀'), Mask(0xFFFF_FFFF_0000_0000));
        assert_eq!(mask('▄'), Mask(0x0000_0000_FFFF_FFFF));
        assert_eq!(mask('▁'), Mask(0xFF));
        assert_eq!(mask('▏'), Mask(0x8080_8080_8080_8080));
        assert_eq!(mask('▕'), Mask(0x0101_0101_0101_0101));
        assert_eq!(mask('▔'), Mask(0xFF00_0000_0000_0000));
        assert_eq!(mask('▘'), Mask(0xF0F0_F0F0_0000_0000));
    }

    #[test]
    fn sextants_split_rows_three_two_three() {
        // BLOCK SEXTANT-1: the upper left third.
        let top_left = mask('\u{1FB00}');
        let rows: Vec<bool> = (0..8).map(|y| top_left.get(0, y)).collect();
        assert_eq!(rows, [true, true, true, false, false, false, false, false]);
        // BLOCK SEXTANT-3: the middle left third.
        let middle = mask('\u{1FB03}');
        assert!(!middle.get(0, 2) && middle.get(0, 3) && middle.get(0, 4) && !middle.get(0, 5));
        assert_eq!(sextant_bits(0x1FB13), Some(0b10100));
        assert_eq!(sextant_bits(0x1FB14), Some(0b10110));
        assert_eq!(sextant_bits(0x1FB3B), Some(0b111110));
    }

    #[test]
    fn octants_fill_the_patterns_that_no_other_character_has() {
        assert_eq!(octant_bits(0x1CD00), Some(0x04));
        assert_eq!(octant_bits(0x1CDE5), Some(0xFE));
        let codes = (OCTANT_FIRST..=OCTANT_LAST).count();
        assert_eq!(codes + OCTANT_ELSEWHERE.len(), 256);
    }

    #[test]
    fn diagonals_fill_the_corner_side_of_their_line() {
        // LOWER LEFT BLOCK DIAGONAL UPPER LEFT TO LOWER CENTRE: a quarter of
        // the cell.
        let shape = Shape::of('\u{1FB40}').expect("supported");
        assert!((shape.coverage_of(0.0, 0.0, 1.0, 1.0) - 0.25).abs() < 1e-5);
        assert!(shape.mask().get(0, 7) && !shape.mask().get(7, 0));
    }

    #[test]
    fn triangles_tile_the_cell() {
        let total: f32 = (0x1FB6C..=0x1FB6F)
            .map(|code| {
                Shape::of(char::from_u32(code).expect("char"))
                    .expect("supported")
                    .coverage_of(0.0, 0.0, 1.0, 1.0)
            })
            .sum();
        assert!((total - 1.0).abs() < 1e-5);
    }

    #[test]
    fn coverage_is_antialiased_at_any_size() {
        let shape = Shape::of('▀').expect("supported");
        let mut out = vec![0; 3 * 3];
        shape.coverage_into(3, 3, &mut out);
        // The middle row is half covered.
        for (value, expected) in out.iter().zip([255u8, 255, 255, 128, 128, 128, 0, 0, 0]) {
            assert!(value.abs_diff(expected) <= 1, "{out:?}");
        }
    }
}
