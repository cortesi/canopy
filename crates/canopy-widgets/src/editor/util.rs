use std::cmp;

use canopy::text::grapheme_width;

/// Compute tab expansion width for a column.
pub fn tab_width(column: usize, tab_stop: usize) -> usize {
    let tab_stop = cmp::max(1, tab_stop);
    let offset = column % tab_stop;
    if offset == 0 {
        tab_stop
    } else {
        tab_stop - offset
    }
}

/// Return the display width of one grapheme at a column, expanding tabs.
pub fn display_width(grapheme: &str, column: usize, tab_stop: usize) -> usize {
    if grapheme == "\t" {
        tab_width(column, tab_stop)
    } else {
        grapheme_width(grapheme)
    }
}
