//! A cursor over a widget's rows, and helpers for measuring and rendering
//! label lists.
//!
//! [`RowCursor`] replaces the near-identical navigation, reveal, and
//! click-to-row code that Selector, Dropdown, and PickerList each copied.
//! [`widest_label`] and [`label_rows`] replace their near-identical width
//! measurement and visible-row iteration.

use canopy::{
    Context, RevealAlign, View,
    event::mouse,
    geom::{PointI32, Rect},
    text,
};

/// A cursor over a fixed count of rows.
///
/// The cursor holds no items of its own: `len` is however many rows a widget
/// currently shows, which can be fewer than its item count when rows are
/// filtered. The current row is `None` exactly when there are no rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowCursor {
    /// Row count the cursor moves over.
    len: usize,
    /// The current row, or `None` when there are no rows.
    index: Option<usize>,
}

impl RowCursor {
    /// Build a cursor over `len` rows, positioned at the first row.
    pub fn new(len: usize) -> Self {
        let mut cursor = Self {
            len: 0,
            index: None,
        };
        cursor.set_len(len);
        cursor
    }

    /// The row count the cursor moves over.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when there are no rows to select.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The current row, or `None` when there are no rows.
    pub fn index(&self) -> Option<usize> {
        self.index
    }

    /// Set the current row directly, clamped to the last row.
    ///
    /// Does nothing when there are no rows.
    pub fn set_index(&mut self, index: usize) {
        if !self.is_empty() {
            self.index = Some(index.min(self.len - 1));
        }
    }

    /// Change the row count, keeping the current row in range.
    ///
    /// A current row past the new end moves back to the last row. A cursor
    /// gaining rows after standing empty opens at the first one.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.index = match (self.index, len) {
            (_, 0) => None,
            (Some(index), len) => Some(index.min(len - 1)),
            (None, _) => Some(0),
        };
    }

    /// Move the current row by a signed offset, saturating at both ends.
    pub fn select_by(&mut self, delta: i32) {
        if self.is_empty() {
            return;
        }
        let current = self.index.unwrap_or(0);
        self.index = Some(
            current
                .saturating_add_signed(delta as isize)
                .min(self.len - 1),
        );
    }

    /// Move to the first row.
    pub fn select_first(&mut self) {
        if !self.is_empty() {
            self.index = Some(0);
        }
    }

    /// Move to the last row.
    pub fn select_last(&mut self) {
        if !self.is_empty() {
            self.index = Some(self.len - 1);
        }
    }

    /// Move by whole pages of `page_rows` visible rows.
    pub fn page(&mut self, delta: i32, page_rows: u32) {
        let page = i32::try_from(page_rows.max(1)).unwrap_or(i32::MAX);
        self.select_by(delta.saturating_mul(page));
    }

    /// Reveal the current row without changing horizontal scroll.
    ///
    /// Does nothing when there is no current row.
    pub fn reveal(&self, c: &mut dyn Context) {
        let Some(index) = self.index else {
            return;
        };
        let Ok(row) = u32::try_from(index) else {
            return;
        };
        c.reveal_area(
            Rect::new(c.view().scroll.x, row, 1, 1),
            RevealAlign::Nearest,
        );
    }

    /// Return the row a click landed on, or `None` outside the rows.
    pub fn row_at(&self, view: &View, location: PointI32) -> Option<usize> {
        let point = view.content_point(location)?;
        let row = point.y as usize;
        (row < self.len).then_some(row)
    }
}

/// True for the mouse-down, left-button click that selects or activates a
/// row.
pub fn is_primary_click(event: mouse::MouseEvent) -> bool {
    event.action == mouse::Action::Down && event.button == mouse::Button::Left
}

/// The widest of `labels`, in display columns, or 0 when there are none.
pub fn widest_label<'a>(labels: impl IntoIterator<Item = &'a str>) -> usize {
    labels
        .into_iter()
        .map(text::display_width)
        .max()
        .unwrap_or(0)
}

/// Rows visible in a view `height` rows tall, starting at `scroll_y`, over
/// `len` total rows.
///
/// Yields `(offset, row)` pairs, where `offset` is the position within the
/// view and `row` the row index, stopping at the height or the row count,
/// whichever comes first.
pub fn label_rows(len: usize, scroll_y: u32, height: u32) -> impl Iterator<Item = (u32, usize)> {
    (scroll_y as usize..len)
        .take(height as usize)
        .enumerate()
        .map(|(offset, row)| (offset as u32, row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_opens_at_the_first_row_or_none_when_empty() {
        assert_eq!(RowCursor::new(0).index(), None);
        assert_eq!(RowCursor::new(3).index(), Some(0));
        assert_eq!(RowCursor::new(3).len(), 3);
        assert!(RowCursor::new(0).is_empty());
        assert!(!RowCursor::new(1).is_empty());
    }

    #[test]
    fn select_by_saturates_at_both_ends() {
        let mut cursor = RowCursor::new(3);
        cursor.select_by(1);
        assert_eq!(cursor.index(), Some(1));
        cursor.select_by(-99);
        assert_eq!(cursor.index(), Some(0));
        cursor.select_by(99);
        assert_eq!(cursor.index(), Some(2));
    }

    #[test]
    fn select_by_on_an_empty_cursor_does_nothing() {
        let mut cursor = RowCursor::new(0);
        cursor.select_by(1);
        assert_eq!(cursor.index(), None);
    }

    #[test]
    fn select_first_and_last_move_to_the_ends() {
        let mut cursor = RowCursor::new(5);
        cursor.set_index(2);
        cursor.select_last();
        assert_eq!(cursor.index(), Some(4));
        cursor.select_first();
        assert_eq!(cursor.index(), Some(0));

        let mut empty = RowCursor::new(0);
        empty.select_first();
        empty.select_last();
        assert_eq!(empty.index(), None);
    }

    #[test]
    fn set_index_clamps_to_the_last_row() {
        let mut cursor = RowCursor::new(3);
        cursor.set_index(99);
        assert_eq!(cursor.index(), Some(2));

        let mut empty = RowCursor::new(0);
        empty.set_index(0);
        assert_eq!(empty.index(), None, "an empty cursor has no rows to select");
    }

    #[test]
    fn page_moves_by_whole_pages() {
        let mut cursor = RowCursor::new(20);
        cursor.page(1, 5);
        assert_eq!(cursor.index(), Some(5));
        cursor.page(1, 5);
        assert_eq!(cursor.index(), Some(10));
        cursor.page(-1, 5);
        assert_eq!(cursor.index(), Some(5));
        cursor.page(99, 5);
        assert_eq!(cursor.index(), Some(19));
    }

    #[test]
    fn page_treats_a_zero_page_size_as_one_row() {
        let mut cursor = RowCursor::new(5);
        cursor.page(2, 0);
        assert_eq!(cursor.index(), Some(2));
    }

    #[test]
    fn set_len_keeps_a_row_in_range_and_reopens_from_empty() {
        let mut cursor = RowCursor::new(5);
        cursor.set_index(4);
        cursor.set_len(2);
        assert_eq!(
            cursor.index(),
            Some(1),
            "a row past the new end moves back to the last row"
        );

        cursor.set_len(0);
        assert_eq!(cursor.index(), None, "no rows means no current row");

        cursor.set_len(3);
        assert_eq!(
            cursor.index(),
            Some(0),
            "gaining rows from empty opens at the first one"
        );

        let mut unchanged = RowCursor::new(5);
        unchanged.set_index(2);
        unchanged.set_len(5);
        assert_eq!(
            unchanged.index(),
            Some(2),
            "an untouched range keeps its row"
        );
    }

    #[test]
    fn label_rows_stops_at_the_height_and_the_row_count() {
        let rows: Vec<_> = label_rows(3, 0, 10).collect();
        assert_eq!(rows, vec![(0, 0), (1, 1), (2, 2)]);

        let rows: Vec<_> = label_rows(10, 0, 3).collect();
        assert_eq!(rows, vec![(0, 0), (1, 1), (2, 2)]);

        let rows: Vec<_> = label_rows(5, 3, 10).collect();
        assert_eq!(
            rows,
            vec![(0, 3), (1, 4)],
            "scroll offsets the starting row"
        );

        let rows: Vec<_> = label_rows(3, 99, 10).collect();
        assert!(rows.is_empty(), "scrolled past the end shows nothing");
    }

    #[test]
    fn widest_label_measures_display_columns() {
        let empty: [&str; 0] = [];
        assert_eq!(widest_label(empty), 0);
        assert_eq!(widest_label(["ab", "abcd", "a"]), 4);
        assert_eq!(widest_label(["日本"]), 4, "wide graphemes count double");
    }
}
