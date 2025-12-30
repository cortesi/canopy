use unicode_width::UnicodeWidthChar;

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

    for (idx, ch) in s.char_indices() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);

        if !started {
            if col + ch_width <= start {
                col += ch_width;
                continue;
            }
            if col < start && col + ch_width > start {
                col += ch_width;
                continue;
            }
            started = true;
            start_byte = idx;
            end_byte = idx;
        }

        if started {
            if ch_width == 0 || out_cols + ch_width <= max {
                out_cols += ch_width;
                end_byte = idx + ch.len_utf8();
            } else {
                break;
            }
        }

        col += ch_width;
        if out_cols >= max {
            break;
        }
    }

    if !started {
        return ("", 0);
    }

    (&s[start_byte..end_byte], out_cols)
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::*;

    #[test]
    fn slice_by_columns_handles_wide_chars() {
        let s = "a界b";
        let (out, width) = slice_by_columns(s, 0, 3);
        assert_eq!(out, "a界");
        assert_eq!(width, UnicodeWidthStr::width(out));

        let (out, width) = slice_by_columns(s, 1, 2);
        assert_eq!(out, "界");
        assert_eq!(width, UnicodeWidthStr::width(out));

        let (out, width) = slice_by_columns(s, 3, 2);
        assert_eq!(out, "b");
        assert_eq!(width, UnicodeWidthStr::width(out));
    }
}
