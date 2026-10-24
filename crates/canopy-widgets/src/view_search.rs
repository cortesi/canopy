//! What every searchable view shares, so a search reads the same in each.
//!
//! A view that highlights search matches, such as the
//! [`Editor`](crate::editor::Editor), the [`DiffView`](crate::DiffView), or a
//! rendered Markdown document, follows four conventions:
//!
//! - It paints the current match with [`CURRENT`] and every other match with
//!   [`MATCH`]. The theme gives both roles one hue, and the current match takes
//!   it at full strength.
//! - It moves between matches with [`step`], which wraps at either end.
//! - It reveals the current match with [`REVEAL`], so the match lands near the
//!   middle of the view with its surroundings on both sides.
//! - It reports one scrollbar mark per row that holds a match, through
//!   [`marks`], from its
//!   [`Widget::scroll_marks`](canopy::Widget::scroll_marks).

use canopy::layout::{RevealAlign, ScrollMark};

use crate::scrollbar::ScrollbarGlyphs;

/// Style role of a match that is not current.
pub const MATCH: &str = "search/match";

/// Style role of the current match.
pub const CURRENT: &str = "search/current";

/// Style role of a scrollbar mark beside a row that holds a match.
///
/// The role carries the match colour as a foreground: a block thumb glyph
/// hides the background, so a mark that slides under the thumb shows its
/// colour only through the foreground.
pub const MARK: &str = "search/mark";

/// Glyph of a match mark on a scrollbar track.
///
/// This is the thin track vertical, so a mark blends into the surrounding
/// divider or border and only its colour stands out.
pub const MARK_GLYPH: char = ScrollbarGlyphs::THIN.track_vertical;

/// How a view reveals the current match: centred, so the text before and
/// after it both show.
pub const REVEAL: RevealAlign = RevealAlign::Center;

/// Return the match `delta` steps from `current` among `count` matches,
/// wrapping at either end.
///
/// With no current match, a forward step counts from before the first match
/// and a backward step from after the last, so `1` gives the first match
/// and `-1` the last. With no matches, there is no current match.
pub fn step(current: Option<usize>, count: usize, delta: i32) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let count = count as i64;
    let from = current.map_or(if delta < 0 { count } else { -1 }, |index| index as i64);
    Some((from + i64::from(delta)).rem_euclid(count) as usize)
}

/// Return one scrollbar mark for each distinct row in `rows`.
///
/// `rows` are canvas rows in ascending order, one per match. Matches that
/// share a row give one mark.
pub fn marks(rows: impl IntoIterator<Item = u32>) -> Vec<ScrollMark> {
    let mut marks: Vec<ScrollMark> = Vec::new();
    for row in rows {
        if marks.last().is_some_and(|mark| mark.start == row) {
            continue;
        }
        marks.push(ScrollMark {
            start: row,
            end: row.saturating_add(1),
            style: MARK,
            glyph: MARK_GLYPH,
        });
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wrap_and_start_outside_the_matches() {
        assert_eq!(step(None, 3, 1), Some(0));
        assert_eq!(step(None, 3, -1), Some(2));
        assert_eq!(step(Some(2), 3, 1), Some(0));
        assert_eq!(step(Some(0), 3, -1), Some(2));
        assert_eq!(step(Some(1), 3, 7), Some(2));
        assert_eq!(step(Some(1), 3, 0), Some(1));
        assert_eq!(step(None, 0, 1), None);
    }

    #[test]
    fn marks_collapse_rows_that_share_matches() {
        let rows = marks([1, 1, 4, 9, 9]);
        assert_eq!(
            rows.iter().map(|mark| mark.start).collect::<Vec<_>>(),
            [1, 4, 9]
        );
        assert!(rows.iter().all(|mark| mark.end == mark.start + 1
            && mark.style == MARK
            && mark.glyph == MARK_GLYPH));
    }
}
