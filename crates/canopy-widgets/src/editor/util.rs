use std::cmp;

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
