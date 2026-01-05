use std::iter::repeat_n;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Slice a string by display columns, returning the substring and its width.
pub fn slice_by_columns(s: &str, start: usize, max: usize) -> (&str, usize) {
    if max == 0 || s.is_empty() {
        return ("", 0);
    }

    let mut col = 0usize;
    let mut out_cols = 0usize;
    let mut started = false;
    let mut start_byte = 0usize;
    let mut end_byte = 0usize;

    for (idx, grapheme) in s.grapheme_indices(true) {
        let g_width = grapheme_width(grapheme);

        if !started {
            if col + g_width <= start {
                col += g_width;
                continue;
            }
            if col < start && col + g_width > start {
                col += g_width;
                continue;
            }
            started = true;
            start_byte = idx;
            end_byte = idx;
        }

        if started {
            if out_cols + g_width <= max {
                out_cols += g_width;
                end_byte = idx + grapheme.len();
            } else {
                break;
            }
        }

        col += g_width;
        if out_cols >= max {
            break;
        }
    }

    if !started {
        return ("", 0);
    }

    (&s[start_byte..end_byte], out_cols)
}

/// Return the display width of a grapheme cluster, clamped to terminal cell widths.
pub fn grapheme_width(grapheme: &str) -> usize {
    if grapheme.is_empty() {
        return 0;
    }
    UnicodeWidthStr::width(grapheme).clamp(1, 2)
}

/// Expand tabs into spaces using the configured tab stop.
pub fn expand_tabs(s: &str, tab_stop: usize) -> String {
    let tab_stop = tab_stop.max(1);
    let mut out = String::new();
    let mut col = 0usize;
    for grapheme in s.graphemes(true) {
        if grapheme == "\t" {
            let width = tab_width(col, tab_stop);
            out.extend(repeat_n(' ', width));
            col = col.saturating_add(width);
            continue;
        }
        if grapheme == "\n" || grapheme == "\r" || grapheme == "\r\n" {
            out.push_str(grapheme);
            col = 0;
            continue;
        }
        out.push_str(grapheme);
        col = col.saturating_add(grapheme_width(grapheme));
    }
    out
}

/// Compute the width of the next tab from the provided column.
fn tab_width(column: usize, tab_stop: usize) -> usize {
    let tab_stop = tab_stop.max(1);
    let offset = column % tab_stop;
    if offset == 0 {
        tab_stop
    } else {
        tab_stop - offset
    }
}

/// Return the display width of a string in terminal cells.
#[cfg(test)]
pub fn display_width(s: &str) -> usize {
    s.graphemes(true).map(grapheme_width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_by_columns_handles_wide_chars() {
        let s = "a界b";
        let (out, width) = slice_by_columns(s, 0, 3);
        assert_eq!(out, "a界");
        assert_eq!(width, display_width(out));

        let (out, width) = slice_by_columns(s, 1, 2);
        assert_eq!(out, "界");
        assert_eq!(width, display_width(out));

        let (out, width) = slice_by_columns(s, 3, 2);
        assert_eq!(out, "b");
        assert_eq!(width, display_width(out));
    }

    #[test]
    fn slice_by_columns_handles_zwj_sequences() {
        let s = "A👩‍💻B";
        let (out, width) = slice_by_columns(s, 0, 3);
        assert_eq!(out, "A👩‍💻");
        assert_eq!(width, 3);

        let (out, width) = slice_by_columns(s, 1, 2);
        assert_eq!(out, "👩‍💻");
        assert_eq!(width, 2);

        let (out, width) = slice_by_columns(s, 2, 2);
        assert_eq!(out, "B");
        assert_eq!(width, 1);
    }

    #[test]
    fn expand_tabs_respects_default_width() {
        assert_eq!(expand_tabs("a\tb", 4), "a   b");
        assert_eq!(expand_tabs("\t", 4), "    ");
    }

    #[test]
    fn expand_tabs_resets_column_on_newlines() {
        assert_eq!(expand_tabs("a\tb\nc\td", 4), "a   b\nc   d");
        assert_eq!(expand_tabs("a\tb\r\nc\td", 4), "a   b\r\nc   d");
    }

    #[test]
    fn expand_tabs_handles_wide_graphemes() {
        assert_eq!(expand_tabs("界\tb", 4), "界  b");
    }
}
