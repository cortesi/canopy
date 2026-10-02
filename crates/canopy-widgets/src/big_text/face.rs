//! The bitmap faces of the big text ladder.

use super::faces;

/// One glyph of a bitmap face: a bitmap placed against the pen and the
/// baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Glyph {
    /// Pixel columns from the pen to the left edge of the bitmap.
    pub(super) x: i8,
    /// Pixel rows from the baseline up to the top edge of the bitmap.
    pub(super) top: i8,
    /// Width of the bitmap in pixels.
    pub(super) width: u8,
    /// Advance of the pen in pixels.
    pub(super) advance: u8,
    /// Rows of the bitmap, top first. Of the `width` low bits of a row, the
    /// most significant is the left pixel.
    pub(super) rows: &'static [u16],
}

impl Glyph {
    /// Returns whether the bitmap has ink at `column` of row `row`.
    pub(super) fn ink(&self, column: u32, row: usize) -> bool {
        let width = u32::from(self.width);
        column < width
            && self
                .rows
                .get(row)
                .is_some_and(|bits| bits >> (width - 1 - column) & 1 == 1)
    }

    /// Returns whether the glyph draws anything.
    pub(super) fn has_ink(&self) -> bool {
        self.rows.iter().any(|bits| *bits != 0)
    }

    /// Returns the rows from the baseline down to the bottom edge of the
    /// bitmap: positive for a descender.
    pub(super) fn descent(&self) -> i32 {
        self.rows.len() as i32 - i32::from(self.top)
    }
}

/// A bitmap face of one size and weight.
#[derive(Debug)]
pub(super) struct Face {
    /// Pixel rows from the baseline up to the top of the capitals and
    /// digits.
    pub(super) cap: u8,
    /// Glyphs, ordered by character.
    pub(super) glyphs: &'static [(char, Glyph)],
}

impl Face {
    /// Returns the glyph of `ch`, if the face has one.
    fn get(&self, ch: char) -> Option<Glyph> {
        self.glyphs
            .binary_search_by_key(&ch, |(key, _)| *key)
            .ok()
            .map(|index| self.glyphs[index].1)
    }
}

/// A face of the big text ladder, smallest first.
///
/// Every face and weight draws one repertoire: printable ASCII, the Latin-1
/// letters and most of its symbols, the light box-drawing lines, and
/// `× ÷ ≤ ≥ ° · — … ↑ ↓ ▲ ▼ ± µ €`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BigFace {
    /// Digits and capitals 5 pixels high: Canopy's own numerals and symbols,
    /// with the letters of Tamzen 5×9.
    Compact,
    /// Tamzen 5×9: digits and capitals 5 pixels high.
    Tamzen5x9,
    /// Tamzen 6×12: digits and capitals 7 pixels high.
    Tamzen6x12,
    /// Tamzen 7×13: digits and capitals 7 pixels high, wider than 6×12.
    Tamzen7x13,
    /// Tamzen 7×14: digits and capitals 8 pixels high.
    Tamzen7x14,
    /// Tamzen 8×15: digits and capitals 8 pixels high, wider than 7×14.
    Tamzen8x15,
    /// Tamzen 8×16: digits and capitals 9 pixels high.
    Tamzen8x16,
    /// Tamzen 10×20: digits and capitals 10 pixels high.
    Tamzen10x20,
}

impl BigFace {
    /// Every face, smallest first. A fit that ties takes the earlier face.
    pub const ALL: [Self; 8] = [
        Self::Compact,
        Self::Tamzen5x9,
        Self::Tamzen6x12,
        Self::Tamzen7x13,
        Self::Tamzen7x14,
        Self::Tamzen8x15,
        Self::Tamzen8x16,
        Self::Tamzen10x20,
    ];

    /// Returns the pixel height of the digits and capitals.
    pub fn cap_height(self) -> u32 {
        u32::from(self.layers(BigWeight::Bold)[0].cap)
    }

    /// Returns the faces that draw this face, first choice first.
    fn layers(self, weight: BigWeight) -> [&'static Face; 2] {
        let tamzen = |bold: &'static Face, regular: &'static Face| match weight {
            BigWeight::Bold => bold,
            BigWeight::Regular => regular,
        };
        let tamzen5x9 = tamzen(&faces::TAMZEN_5X9_BOLD, &faces::TAMZEN_5X9_REGULAR);
        let face = match self {
            Self::Compact => return [&faces::COMPACT, tamzen5x9],
            Self::Tamzen5x9 => tamzen5x9,
            Self::Tamzen6x12 => tamzen(&faces::TAMZEN_6X12_BOLD, &faces::TAMZEN_6X12_REGULAR),
            Self::Tamzen7x13 => tamzen(&faces::TAMZEN_7X13_BOLD, &faces::TAMZEN_7X13_REGULAR),
            Self::Tamzen7x14 => tamzen(&faces::TAMZEN_7X14_BOLD, &faces::TAMZEN_7X14_REGULAR),
            Self::Tamzen8x15 => tamzen(&faces::TAMZEN_8X15_BOLD, &faces::TAMZEN_8X15_REGULAR),
            Self::Tamzen8x16 => tamzen(&faces::TAMZEN_8X16_BOLD, &faces::TAMZEN_8X16_REGULAR),
            Self::Tamzen10x20 => tamzen(&faces::TAMZEN_10X20_BOLD, &faces::TAMZEN_10X20_REGULAR),
        };
        [face, face]
    }

    /// Returns the glyph that draws `ch` in `weight`. Aliases draw as their
    /// plain forms, other whitespace as a space, and a character outside the
    /// repertoire as `?`.
    pub(super) fn glyph(self, weight: BigWeight, ch: char) -> Glyph {
        let ch = match ch {
            '−' | '–' => '-',
            '‘' | '’' => '\'',
            '“' | '”' => '"',
            ch if ch.is_whitespace() => ' ',
            ch => ch,
        };
        let layers = self.layers(weight);
        layers
            .iter()
            .find_map(|face| face.get(ch))
            .or_else(|| layers.iter().find_map(|face| face.get('?')))
            .unwrap_or(Glyph {
                x: 0,
                top: 0,
                width: 0,
                advance: 0,
                rows: &[],
            })
    }
}

/// The weight of big text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BigWeight {
    /// Strokes two pixels wide where the face has them. Numbers read best
    /// bold at these sizes, so this is the default.
    #[default]
    Bold,
    /// Strokes one pixel wide. The compact face keeps its numerals.
    Regular,
}
