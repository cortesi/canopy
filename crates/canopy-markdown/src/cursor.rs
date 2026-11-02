//! The movement of the block cursor over the rows of edit units.
//!
//! Each unit has an extent: the content rows of its blocks in the shown
//! presentation, from the first row of its first block to the last row of its
//! last. A unit with no rows has an empty extent, and the cursor passes over
//! it.

use std::ops::Range;

/// The document rows that a view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The first row in view.
    pub top: u32,
    /// The rows in view.
    pub height: u32,
    /// The rows of the document.
    pub canvas: u32,
}

impl Frame {
    /// Return the row after the last row in view.
    fn bottom(self) -> u32 {
        self.top.saturating_add(self.height)
    }

    /// Return `top` moved into the range the view can scroll to.
    fn clamp(self, top: u32) -> u32 {
        top.min(self.canvas.saturating_sub(self.height))
    }

    /// Return whether the view shows a row of `rows`.
    pub fn shows(self, rows: &Range<u32>) -> bool {
        !rows.is_empty() && rows.start < self.bottom() && rows.end > self.top
    }
}

/// A step of the cursor: the unit it marks, and the first row of the view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// The index of the marked unit.
    pub unit: usize,
    /// The first row in view after the step.
    pub top: u32,
}

/// Return the first row of a view that shows `rows`. A unit that fits moves
/// into view by the smallest move. A taller unit shows its top at the top of
/// the view when the cursor enters it going `down`, and its bottom at the
/// bottom of the view otherwise.
pub fn reveal(rows: &Range<u32>, frame: Frame, down: bool) -> u32 {
    let top = if rows.len() as u32 <= frame.height {
        if rows.start < frame.top {
            rows.start
        } else if rows.end > frame.bottom() {
            rows.end - frame.height
        } else {
            frame.top
        }
    } else if down {
        rows.start
    } else {
        rows.end - frame.height
    };
    frame.clamp(top)
}

/// Return the step that the scroll of a unit takes: a page less two rows,
/// and at least one row.
fn page(frame: Frame) -> u32 {
    frame.height.saturating_sub(2).max(1)
}

/// Step down from unit `marked`. While the unit continues below the view,
/// scroll toward its end and stay. Otherwise mark the next unit with rows.
/// Return `None` when nothing moves.
pub fn down(extents: &[Range<u32>], marked: usize, frame: Frame) -> Option<Step> {
    let rows = extents.get(marked)?;
    if rows.end > frame.bottom() {
        let top = frame
            .top
            .saturating_add(page(frame))
            .min(rows.end - frame.height);
        return Some(Step {
            unit: marked,
            top: frame.clamp(top),
        });
    }
    let next = (marked + 1..extents.len()).find(|&index| !extents[index].is_empty())?;
    Some(Step {
        unit: next,
        top: reveal(&extents[next], frame, true),
    })
}

/// Step up from unit `marked`. While the unit continues above the view,
/// scroll toward its start and stay. Otherwise mark the previous unit with
/// rows. Return `None` when nothing moves.
pub fn up(extents: &[Range<u32>], marked: usize, frame: Frame) -> Option<Step> {
    let rows = extents.get(marked)?;
    if rows.start < frame.top {
        let top = frame.top.saturating_sub(page(frame)).max(rows.start);
        return Some(Step { unit: marked, top });
    }
    let previous = (0..marked.min(extents.len()))
        .rev()
        .find(|&index| !extents[index].is_empty())?;
    Some(Step {
        unit: previous,
        top: reveal(&extents[previous], frame, false),
    })
}

/// Mark the first unit with rows, and show its top.
pub fn first(extents: &[Range<u32>], frame: Frame) -> Option<Step> {
    let unit = extents.iter().position(|rows| !rows.is_empty())?;
    Some(Step {
        unit,
        top: reveal(&extents[unit], frame, true),
    })
}

/// Mark the last unit with rows, and show its bottom.
pub fn last(extents: &[Range<u32>], frame: Frame) -> Option<Step> {
    let unit = extents.iter().rposition(|rows| !rows.is_empty())?;
    Some(Step {
        unit,
        top: reveal(&extents[unit], frame, false),
    })
}

/// Keep the cursor on a unit with a row in view after the view moved
/// freely, as a page key or the wheel moves it. The marked unit stays while
/// it shows. Otherwise the cursor marks the first unit in view, or with none
/// in view, the nearest unit below the view, else the last unit, and the view
/// moves to show it. Return the step, or `None` when the cursor stays.
pub fn settle(extents: &[Range<u32>], marked: Option<usize>, frame: Frame) -> Option<Step> {
    if marked
        .and_then(|index| extents.get(index))
        .is_some_and(|rows| frame.shows(rows))
    {
        return None;
    }
    if let Some(unit) = in_view(extents, frame) {
        return Some(Step {
            unit,
            top: frame.top,
        });
    }
    let unit = extents
        .iter()
        .position(|rows| !rows.is_empty() && rows.start >= frame.top)
        .or_else(|| extents.iter().rposition(|rows| !rows.is_empty()))?;
    let rows = &extents[unit];
    Some(Step {
        unit,
        top: reveal(rows, frame, rows.start >= frame.top),
    })
}

/// Return the first unit with a row in view.
pub fn in_view(extents: &[Range<u32>], frame: Frame) -> Option<usize> {
    extents.iter().position(|rows| frame.shows(rows))
}

/// Return the unit whose extent holds `row`.
pub fn at_row(extents: &[Range<u32>], row: u32) -> Option<usize> {
    extents.iter().position(|rows| rows.contains(&row))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view of `height` rows from `top`, over 100 rows.
    fn frame(top: u32, height: u32) -> Frame {
        Frame {
            top,
            height,
            canvas: 100,
        }
    }

    #[test]
    fn a_unit_that_fits_moves_into_view_by_the_smallest_move() {
        assert_eq!(reveal(&(2..4), frame(10, 5), true), 2);
        assert_eq!(reveal(&(20..24), frame(10, 5), false), 19);
        assert_eq!(reveal(&(11..13), frame(10, 5), true), 10);
        // A view cannot scroll past the end of the document.
        assert_eq!(
            reveal(
                &(98..100),
                Frame {
                    top: 0,
                    height: 5,
                    canvas: 100
                },
                true
            ),
            95
        );
    }

    #[test]
    fn a_tall_unit_shows_the_end_it_enters_from() {
        assert_eq!(reveal(&(20..40), frame(0, 5), true), 20);
        assert_eq!(reveal(&(20..40), frame(50, 5), false), 35);
    }

    #[test]
    fn down_pages_through_a_tall_unit_before_it_moves_on() {
        let extents = [0..2, 3..20, 21..22];
        let mut view = frame(3, 6);
        let mut tops = Vec::new();
        while let Some(step) = down(&extents, 1, view).filter(|step| step.unit == 1) {
            tops.push(step.top);
            view.top = step.top;
        }
        // Four rows a step, and the last step only as far as the end.
        assert_eq!(tops, [7, 11, 14]);
        assert_eq!(down(&extents, 1, view), Some(Step { unit: 2, top: 16 }));
        assert_eq!(down(&extents, 2, frame(16, 6)), None);
    }

    #[test]
    fn up_mirrors_down() {
        let extents = [0..2, 3..20, 21..22];
        assert_eq!(
            up(&extents, 1, frame(14, 6)),
            Some(Step { unit: 1, top: 10 })
        );
        assert_eq!(up(&extents, 1, frame(5, 6)), Some(Step { unit: 1, top: 3 }));
        assert_eq!(up(&extents, 1, frame(3, 6)), Some(Step { unit: 0, top: 0 }));
        assert_eq!(up(&extents, 0, frame(0, 6)), None);
        // A tall unit entered going up shows its bottom.
        assert_eq!(
            up(&extents, 2, frame(20, 6)),
            Some(Step { unit: 1, top: 14 })
        );
    }

    #[test]
    fn a_view_of_one_row_still_steps() {
        let extents = [0..3, 4..5];
        assert_eq!(
            down(&extents, 0, frame(0, 1)),
            Some(Step { unit: 0, top: 1 })
        );
        assert_eq!(
            down(&extents, 0, frame(2, 1)),
            Some(Step { unit: 1, top: 4 })
        );
        assert_eq!(up(&extents, 0, frame(2, 1)), Some(Step { unit: 0, top: 1 }));
    }

    #[test]
    fn units_without_rows_are_passed_over() {
        let extents = [0..2, 2..2, 4..6];
        assert_eq!(
            down(&extents, 0, frame(0, 10)).map(|step| step.unit),
            Some(2)
        );
        assert_eq!(up(&extents, 2, frame(0, 10)).map(|step| step.unit), Some(0));
        assert_eq!(
            first(&[1..1, 2..3], frame(0, 5)).map(|step| step.unit),
            Some(1)
        );
        assert_eq!(
            last(&[1..2, 3..3], frame(0, 5)).map(|step| step.unit),
            Some(0)
        );
        assert_eq!(first(&[], frame(0, 5)), None);
    }

    #[test]
    fn a_free_scroll_keeps_the_cursor_on_a_unit_in_view() {
        let extents = [0..2, 4..5, 8..9];
        // The marked unit shows, so it stays.
        assert_eq!(settle(&extents, Some(1), frame(3, 3)), None);
        // It went out of view, so the first unit in view takes the cursor.
        assert_eq!(
            settle(&extents, Some(0), frame(3, 3)),
            Some(Step { unit: 1, top: 3 })
        );
        assert_eq!(
            settle(&extents, None, frame(0, 3)),
            Some(Step { unit: 0, top: 0 })
        );
        // No unit shows, so the view moves to the nearest below it.
        assert_eq!(
            settle(&extents, Some(0), frame(5, 2)),
            Some(Step { unit: 2, top: 7 })
        );
        // Below the last unit, the view moves back to it.
        assert_eq!(
            settle(&extents, Some(0), frame(9, 2)),
            Some(Step { unit: 2, top: 8 })
        );
        assert_eq!(settle(&[], None, frame(0, 2)), None);
    }

    #[test]
    fn the_view_and_rows_find_their_units() {
        let extents = [0..2, 3..6, 8..9];
        assert_eq!(in_view(&extents, frame(2, 2)), Some(1));
        assert_eq!(in_view(&extents, frame(6, 2)), None);
        assert_eq!(at_row(&extents, 5), Some(1));
        assert_eq!(at_row(&extents, 7), None);
        assert!(!frame(0, 5).shows(&(3..3)));
    }
}
