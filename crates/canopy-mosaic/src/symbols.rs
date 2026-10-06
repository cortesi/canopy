//! Families of mosaic characters, the tiers that group them, and the
//! compiled sets that the matcher searches.

use std::{cmp::Ordering, collections::HashMap, sync::OnceLock};

use crate::geometry::{
    Mask, OCTANT_ELSEWHERE, OCTANT_FIRST, OCTANT_LAST, SEXTANT_FIRST, SEXTANT_LAST, Shape,
};

/// A group of characters that terminals and fonts tend to draw alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// The space.
    Space,
    /// The full block and the upper and lower halves.
    Halves,
    /// The left and right halves and the quadrants.
    Quadrants,
    /// The other block elements: eighths from each edge, without the shades.
    Eighths,
    /// The octants, and the quarter blocks that complete their patterns.
    Octants,
    /// The sextants.
    Sextants,
    /// The one-eighth bars and the quarter blocks of Symbols for Legacy
    /// Computing.
    Bars,
    /// The smooth mosaics and the triangles that meet at the center.
    Diagonals,
}

impl Family {
    /// Every family, in priority order: on an exact tie, an earlier family
    /// wins, because more terminals draw it well.
    pub const ALL: [Self; 8] = [
        Self::Space,
        Self::Halves,
        Self::Quadrants,
        Self::Eighths,
        Self::Octants,
        Self::Sextants,
        Self::Bars,
        Self::Diagonals,
    ];

    /// The characters of the family, in code point order.
    pub fn chars(self) -> Vec<char> {
        let range = |first: u32, last: u32| (first..=last).filter_map(char::from_u32);
        match self {
            Self::Space => vec![' '],
            Self::Halves => vec!['▀', '▄', '█'],
            Self::Quadrants => range(0x2596, 0x259F).chain(['▌', '▐']).collect(),
            Self::Eighths => range(0x2581, 0x2583)
                .chain(range(0x2585, 0x2587))
                .chain(range(0x2589, 0x258B))
                .chain(range(0x258D, 0x258F))
                .chain(['▔', '▕'])
                .collect(),
            Self::Octants => range(OCTANT_FIRST, OCTANT_LAST)
                .chain(['\u{1CEA0}', '\u{1CEA3}', '\u{1CEA8}', '\u{1CEAB}'])
                .chain(['\u{1FBE6}', '\u{1FBE7}'])
                .collect(),
            Self::Sextants => range(SEXTANT_FIRST, SEXTANT_LAST).collect(),
            Self::Bars => range(0x1FB70, 0x1FB8B).collect(),
            Self::Diagonals => range(0x1FB3C, 0x1FB6F).collect(),
        }
    }
}

/// A cumulative set of families: each tier holds the one before it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// Half blocks: two pixels stacked in a cell.
    Halves,
    /// Quadrants: four pixels in a cell.
    Quadrants,
    /// Every block element except the shades. Nearly every terminal font has
    /// them, and many terminals draw them from geometry.
    Blocks,
    /// The block elements, plus the octants, sextants, bars, and smooth
    /// mosaics of Symbols for Legacy Computing. It draws the best pictures.
    /// Some fonts lack these mosaics.
    #[default]
    Full,
}

impl Tier {
    /// Every tier, from the fewest symbols to the most.
    pub const ALL: [Self; 4] = [Self::Halves, Self::Quadrants, Self::Blocks, Self::Full];

    /// The families of the tier, in priority order.
    pub fn families(self) -> &'static [Family] {
        let all = &Family::ALL;
        match self {
            Self::Halves => &all[..2],
            Self::Quadrants => &all[..3],
            Self::Blocks => &all[..4],
            Self::Full => all,
        }
    }

    /// The name of the tier, in lower case.
    pub fn name(self) -> &'static str {
        match self {
            Self::Halves => "halves",
            Self::Quadrants => "quadrants",
            Self::Blocks => "blocks",
            Self::Full => "full",
        }
    }

    /// The tier after this one, wrapping from the last to the first.
    #[must_use]
    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|tier| *tier == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// The tier with a name, as [`Self::name`] gives it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.name() == name)
    }
}

/// One symbol of a set: a mask, and the character that draws it.
///
/// A mask and its inverse are one symbol with the pens swapped, so a set
/// holds one entry for each pair. The entry's mask is the canonical one of
/// the pair: the one with fewer set pixels, or the smaller on a tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The canonical mask.
    pub mask: Mask,
    /// The character that the matcher emits for this entry.
    pub ch: char,
    /// Whether `ch` draws the inverse of `mask` as its ink, so that the
    /// foreground pen colors the clear pixels of `mask`.
    pub inverted: bool,
    /// The family of the entry, the earliest family with either mask.
    pub family: Family,
}

/// A compiled set of symbols, in priority order.
#[derive(Debug, Clone)]
pub struct SymbolSet {
    /// The entries, in priority order.
    entries: Vec<Entry>,
}

impl SymbolSet {
    /// The compiled set of a built-in tier. Each tier compiles once, on first
    /// use.
    pub fn tier(tier: Tier) -> &'static Self {
        static SETS: [OnceLock<SymbolSet>; 4] = [const { OnceLock::new() }; 4];
        let index = Tier::ALL.iter().position(|t| *t == tier).unwrap_or(0);
        SETS[index].get_or_init(|| Self::from_families(tier.families()))
    }

    /// Compile a set from families, in priority order.
    pub fn from_families(families: &[Family]) -> Self {
        /// The characters found for one canonical mask.
        struct Found {
            /// Position of the entry in priority order.
            order: usize,
            /// The family that first gave the mask.
            family: Family,
            /// The first character with the canonical mask as its ink.
            ink: Option<char>,
            /// The first character with the inverse mask as its ink.
            inverse: Option<char>,
        }
        let mut found: HashMap<Mask, Found> = HashMap::new();
        for &family in families {
            for ch in family.chars() {
                let Some(shape) = Shape::of(ch) else {
                    continue;
                };
                let mask = shape.mask();
                let canonical = canonical(mask);
                let order = found.len();
                let slot = found.entry(canonical).or_insert(Found {
                    order,
                    family,
                    ink: None,
                    inverse: None,
                });
                let place = if mask == canonical {
                    &mut slot.ink
                } else {
                    &mut slot.inverse
                };
                place.get_or_insert(ch);
            }
        }
        let mut entries: Vec<(usize, Entry)> = found
            .into_iter()
            .map(|(mask, found)| {
                let (ch, inverted) = match (found.ink, found.inverse) {
                    // Prefer the character whose ink covers less, so its
                    // ground covers at least half the cell. On a tie, take
                    // the lower code point.
                    (Some(ink), Some(inverse)) if mask.count() == 32 && inverse < ink => {
                        (inverse, true)
                    }
                    (Some(ink), _) => (ink, false),
                    (None, Some(inverse)) => (inverse, true),
                    (None, None) => unreachable!("an entry has a character"),
                };
                (
                    found.order,
                    Entry {
                        mask,
                        ch,
                        inverted,
                        family: found.family,
                    },
                )
            })
            .collect();
        entries.sort_by_key(|(order, _)| *order);
        Self {
            entries: entries.into_iter().map(|(_, entry)| entry).collect(),
        }
    }

    /// The entries, in priority order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The characters that the set can emit, in priority order.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.entries.iter().map(|entry| entry.ch)
    }
}

/// Of a mask and its inverse, the one with fewer set pixels, or the smaller
/// on a tie.
pub fn canonical(mask: Mask) -> Mask {
    let inverse = mask.inverse();
    match mask.count().cmp(&inverse.count()) {
        Ordering::Less => mask,
        Ordering::Greater => inverse,
        Ordering::Equal => mask.min(inverse),
    }
}

/// The character that draws a 2×4 octant pattern. Bit `n` is octant `n + 1`
/// in Unicode's numbering: left to right, then down. Every pattern has a
/// character, from the octant block or elsewhere.
pub fn octant(bits: u8) -> char {
    if let Some((_, ch)) = OCTANT_ELSEWHERE
        .iter()
        .find(|(pattern, _)| *pattern == bits)
    {
        return *ch;
    }
    let before = OCTANT_ELSEWHERE
        .iter()
        .filter(|(pattern, _)| *pattern < bits)
        .count() as u32;
    char::from_u32(OCTANT_FIRST + u32::from(bits) - before).unwrap_or(' ')
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn tiers_hold_the_planned_characters() {
        let count = |tier: Tier| -> usize {
            tier.families()
                .iter()
                .map(|family| family.chars().len())
                .sum()
        };
        assert_eq!(count(Tier::Halves), 4);
        assert_eq!(count(Tier::Quadrants), 16);
        assert_eq!(count(Tier::Blocks), 30);
        assert_eq!(count(Tier::Full), 406);
    }

    #[test]
    fn sets_fold_each_mask_with_its_inverse() {
        assert_eq!(SymbolSet::tier(Tier::Halves).len(), 2);
        assert_eq!(SymbolSet::tier(Tier::Quadrants).len(), 8);
        assert_eq!(SymbolSet::tier(Tier::Blocks).len(), 20);
        for tier in Tier::ALL {
            let set = SymbolSet::tier(tier);
            let masks: HashSet<Mask> = set.entries().iter().map(|e| e.mask).collect();
            assert_eq!(masks.len(), set.len(), "{tier:?}");
            for entry in set.entries() {
                assert_eq!(canonical(entry.mask), entry.mask);
                let drawn = Shape::of(entry.ch).expect("supported").mask();
                let expected = if entry.inverted {
                    entry.mask.inverse()
                } else {
                    entry.mask
                };
                assert_eq!(drawn, expected, "{tier:?} {:?}", entry.ch);
            }
        }
    }

    #[test]
    fn the_empty_mask_draws_as_a_space_and_halves_as_the_upper_half() {
        let set = SymbolSet::tier(Tier::Halves);
        assert_eq!(set.entries()[0].ch, ' ');
        assert_eq!(set.entries()[0].mask, Mask::EMPTY);
        assert_eq!(set.entries()[1].ch, '▀');
    }

    #[test]
    fn every_octant_pattern_has_a_distinct_character() {
        let chars: HashSet<char> = (0..=255u8).map(octant).collect();
        assert_eq!(chars.len(), 256);
        for bits in 0..=255u8 {
            let ch = octant(bits);
            let expected = Mask::from_fn(|x, y| bits & (1 << ((y / 2) * 2 + x / 4)) != 0);
            assert_eq!(
                Shape::of(ch).expect("supported").mask(),
                expected,
                "{bits:#x}"
            );
        }
    }

    #[test]
    fn tiers_cycle_and_name_themselves() {
        assert_eq!(Tier::Full.next(), Tier::Halves);
        for tier in Tier::ALL {
            assert_eq!(Tier::from_name(tier.name()), Some(tier));
        }
    }
}
