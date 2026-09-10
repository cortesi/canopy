use canopy::text::{grapheme_width, tab_width};
use unicode_segmentation::UnicodeSegmentation;

/// Return the display width of one grapheme at a column, expanding tabs.
pub fn display_width(grapheme: &str, column: usize, tab_stop: usize) -> usize {
    if grapheme == "\t" {
        tab_width(column, tab_stop)
    } else {
        grapheme_width(grapheme)
    }
}

/// Replace newline characters with spaces for single-line text.
pub fn single_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// Return the grapheme boundary immediately before a char column, or 0.
pub fn prev_grapheme_boundary(line: &str, column: usize) -> usize {
    let mut previous = 0usize;
    let mut count = 0usize;
    for grapheme in line.graphemes(true) {
        if count >= column {
            break;
        }
        previous = count;
        count = count.saturating_add(grapheme.chars().count());
    }
    if count < column { count } else { previous }
}

/// Return the grapheme boundary immediately after a char column, or the column
/// itself.
pub fn next_grapheme_boundary(line: &str, column: usize) -> usize {
    let mut count = 0usize;
    for grapheme in line.graphemes(true) {
        let next = count.saturating_add(grapheme.chars().count());
        if column < next {
            return next;
        }
        count = next;
    }
    column
}
