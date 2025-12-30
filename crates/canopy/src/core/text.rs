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
}
