//! Utilities for working with TermBufs in tests.
use crate::{geom::Point, style::PartialStyle, termbuf::TermBuf};

/// A helper macro to create buffers for the termbuf match assertions.
#[macro_export]
macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
}

/// A struct for configuring buffer matching behavior. By default, it treats 'X' as a special
/// marker for NULL cells in the buffer, allowing us to test partial renders.
pub struct BufMatch {
    null_char: char,
    any_char: Option<char>,
}

impl Default for BufMatch {
    fn default() -> Self {
        Self {
            null_char: 'X',
            any_char: None,
        }
    }
}

impl BufMatch {
    /// Create a new BufMatch with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the character used to match NULL cells in the buffer.
    /// Default is 'X'.
    pub fn with_null(mut self, null_char: char) -> Self {
        self.null_char = null_char;
        self
    }

    /// Set a character that matches any character in the buffer.
    /// When set, this character in the expected pattern will match any character in the actual buffer.
    pub fn with_any(mut self, any_char: char) -> Self {
        self.any_char = Some(any_char);
        self
    }

    /// Returns true if the buffer content matches the expected lines.
    pub fn matches(&self, buf: &TermBuf, expected: &[&str]) -> bool {
        if expected.len() != buf.size().h as usize {
            return false;
        }

        for (y, expected_line) in expected.iter().enumerate() {
            // Get actual line character by character to handle NULL cells
            let mut actual_chars = Vec::new();
            for x in 0..buf.size().w {
                if let Some(cell) = buf.get(Point { x, y: y as u32 }) {
                    if cell.ch == '\0' {
                        actual_chars.push(self.null_char);
                    } else {
                        actual_chars.push(cell.ch);
                    }
                }
            }
            let actual_line: String = actual_chars.into_iter().collect();

            // Compare lines character by character to handle any_char
            let expected_trimmed = expected_line.trim_end();
            let actual_trimmed = actual_line.trim_end();

            if expected_trimmed.len() != actual_trimmed.len() {
                return false;
            }

            for (expected_ch, actual_ch) in expected_trimmed.chars().zip(actual_trimmed.chars()) {
                if let Some(any) = self.any_char {
                    if expected_ch == any {
                        continue; // any_char matches anything
                    }
                }
                if expected_ch != actual_ch {
                    return false;
                }
            }
        }

        true
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure.
    pub fn assert_matches(&self, buf: &TermBuf, expected: &[&str]) {
        self.assert_matches_with_context(buf, expected, None);
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure,
    /// with optional context information.
    pub fn assert_matches_with_context(
        &self,
        buf: &TermBuf,
        expected: &[&str],
        context: Option<&str>,
    ) {
        if !self.matches(buf, expected) {
            let actual_lines = buf.lines();
            let width = expected.first().map(|l| l.len()).unwrap_or(10).max(10);

            if let Some(ctx) = context {
                println!("\n{ctx}");
            }

            println!("\nExpected:");
            println!("┌{}┐", "─".repeat(width));
            for line in expected {
                println!("│{line:width$}│");
            }
            println!("└{}┘", "─".repeat(width));

            println!("\nActual:");
            println!("┌{}┐", "─".repeat(width));
            for line in &actual_lines {
                println!("│{line:width$}│");
            }
            println!("└{}┘", "─".repeat(width));

            panic!("Buffer contents did not match expected pattern");
        }
    }
}

/// Returns true if the buffer content matches the expected lines.
/// This is the non-panicking version of assert_buffer_matches.
///
/// In expected lines, the character 'X' is treated as a special marker for NULL cells.
/// NULL cells (containing '\0') in the actual buffer will match 'X' in the expected pattern.
pub fn buffer_matches(buf: &TermBuf, expected: &[&str]) -> bool {
    BufMatch::default().matches(buf, expected)
}

/// Assert that the buffer matches the expected lines with pretty printed output on failure.
///
/// In expected lines, the character 'X' is treated as a special marker for NULL cells.
/// NULL cells (containing '\0') in the actual buffer will match 'X' in the expected pattern.
/// This is useful for testing partial renders where some areas remain uninitialized.
pub fn assert_matches(buf: &TermBuf, expected: &[&str]) {
    BufMatch::default().assert_matches(buf, expected);
}

/// Assert that the buffer matches the expected lines with pretty printed output on failure,
/// with optional context information.
///
/// In expected lines, the character 'X' is treated as a special marker for NULL cells.
/// NULL cells (containing '\0') in the actual buffer will match 'X' in the expected pattern.
///
/// The context parameter allows providing additional information that will be displayed
/// before the expected/actual comparison if the assertion fails.
pub fn assert_matches_with_context(buf: &TermBuf, expected: &[&str], context: Option<&str>) {
    BufMatch::default().assert_matches_with_context(buf, expected, context);
}

/// Does the buffer contain the supplied substring?
pub fn contains_text(buf: &TermBuf, txt: &str) -> bool {
    buf.lines().iter().any(|l| l.contains(txt))
}

/// Does the buffer contain the supplied substring in the given foreground colour?
pub fn contains_text_fg(buf: &TermBuf, txt: &str, fg: crate::style::Color) -> bool {
    contains_text_style(buf, txt, &PartialStyle::fg(fg))
}

/// Does the buffer contain the supplied substring with the given style?
pub fn contains_text_style(buf: &TermBuf, txt: &str, style: &PartialStyle) -> bool {
    let tl = txt.chars().count() as u32;
    if tl == 0 || tl > buf.size().w {
        return false;
    }
    for y in 0..buf.size().h {
        for x in 0..=buf.size().w.saturating_sub(tl) {
            let mut m = true;
            let mut c = false;
            for (i, ch) in txt.chars().enumerate() {
                if let Some(cell) = buf.get(Point { x: x + i as u32, y }) {
                    if cell.ch != ch {
                        m = false;
                        break;
                    }
                    // Check if the cell style matches the partial style
                    let style_matches = (style.fg.is_none() || style.fg == Some(cell.style.fg))
                        && (style.bg.is_none() || style.bg == Some(cell.style.bg))
                        && (style.attrs.is_none() || style.attrs == Some(cell.style.attrs));
                    if style_matches {
                        c = true;
                    }
                } else {
                    m = false;
                    break;
                }
            }
            if m && c {
                return true;
            }
        }
    }
    false
}

/// Dumps the contents of a TermBuf to the terminal for debugging purposes.
/// This is useful for visualizing buffer contents directly in test output.
/// Includes rulers on the bottom and right side for easy offset visualization.
pub fn dump(buf: &TermBuf) {
    let width = buf.size().w as usize;

    println!("\nTermBuf dump ({}x{}):", buf.size().w, buf.size().h);
    println!("┌{}┐", "─".repeat(width));

    for y in 0..buf.size().h {
        print!("│");
        for x in 0..buf.size().w {
            if let Some(cell) = buf.get(Point { x, y }) {
                if cell.ch == '\0' {
                    print!("·");
                } else {
                    print!("{}", cell.ch);
                }
            }
        }
        println!("│{}", y % 10);
    }

    println!("└{}┘", "─".repeat(width));

    // Bottom ruler
    print!(" ");
    for x in 0..width {
        print!("{}", x % 10);
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::Expanse,
        style::{AttrSet, Color},
    };

    fn test_style() -> crate::style::Style {
        crate::style::Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn test_bufmatch_default() {
        let mut buf = TermBuf::empty(Expanse::new(5, 3));
        buf.text(test_style(), crate::geom::Line::new(0, 0, 5), "hello");

        let matcher = BufMatch::default();
        assert!(matcher.matches(&buf, &["hello", "XXXXX", "XXXXX"]));
        assert!(!matcher.matches(&buf, &["world", "XXXXX", "XXXXX"]));
    }

    #[test]
    fn test_bufmatch_custom_null() {
        let mut buf = TermBuf::empty(Expanse::new(4, 2));
        buf.text(test_style(), crate::geom::Line::new(0, 0, 2), "ab");

        let matcher = BufMatch::new().with_null('_');
        assert!(matcher.matches(&buf, &["ab__", "____"]));
        assert!(!matcher.matches(&buf, &["abXX", "XXXX"])); // 'X' is not the null char anymore
    }

    #[test]
    fn test_bufmatch_any_char() {
        let mut buf = TermBuf::new(Expanse::new(4, 2), ' ', test_style());
        buf.text(test_style(), crate::geom::Line::new(0, 0, 4), "test");
        buf.text(test_style(), crate::geom::Line::new(0, 1, 4), "word");

        let matcher = BufMatch::new().with_any('?');
        assert!(matcher.matches(&buf, &["????", "????"])); // all wildcards
        assert!(matcher.matches(&buf, &["te??", "wo??"])); // partial wildcards
        assert!(matcher.matches(&buf, &["test", "word"])); // exact match still works
        assert!(!matcher.matches(&buf, &["fail", "word"])); // wrong text
    }

    #[test]
    fn test_bufmatch_combined() {
        let mut buf = TermBuf::empty(Expanse::new(6, 2));
        buf.text(test_style(), crate::geom::Line::new(0, 0, 3), "foo");

        let matcher = BufMatch::new().with_null('_').with_any('*');
        assert!(matcher.matches(&buf, &["foo___", "______"])); // custom null char
        assert!(matcher.matches(&buf, &["***___", "******"])); // any + null
        assert!(matcher.matches(&buf, &["f**___", "______"])); // mixed
    }

    #[test]
    fn test_contains_functions() {
        let mut buf = TermBuf::new(Expanse::new(10, 2), ' ', test_style());

        let mut red_style = test_style();
        red_style.fg = Color::Red;

        buf.text(test_style(), crate::geom::Line::new(0, 0, 5), "hello");
        buf.text(red_style, crate::geom::Line::new(5, 0, 5), "world");

        assert!(contains_text(&buf, "hello"));
        assert!(contains_text(&buf, "world"));
        assert!(!contains_text(&buf, "goodbye"));

        assert!(contains_text_fg(&buf, "world", Color::Red));
        assert!(!contains_text_fg(&buf, "hello", Color::Red));

        assert!(contains_text_style(
            &buf,
            "world",
            &PartialStyle::fg(Color::Red)
        ));
        assert!(contains_text_style(
            &buf,
            "hello",
            &PartialStyle::fg(Color::White)
        ));
    }

    #[test]
    fn test_dump() {
        let mut buf = TermBuf::empty(Expanse::new(5, 3));
        buf.text(test_style(), crate::geom::Line::new(0, 0, 5), "hello");
        buf.text(test_style(), crate::geom::Line::new(1, 1, 3), "abc");

        // This test just verifies dump() runs without panicking
        // The actual output goes to stdout
        dump(&buf);
    }

    #[test]
    fn test_dump_with_larger_buffer() {
        // Test with a larger buffer to see the ruler wrap around
        let mut buf = TermBuf::empty(Expanse::new(25, 15));
        buf.text(test_style(), crate::geom::Line::new(0, 0, 10), "0123456789");
        buf.text(
            test_style(),
            crate::geom::Line::new(10, 5, 15),
            "Offset at (10,5)",
        );
        buf.text(
            test_style(),
            crate::geom::Line::new(5, 10, 10),
            "Row 10 test",
        );

        dump(&buf);
    }
}
