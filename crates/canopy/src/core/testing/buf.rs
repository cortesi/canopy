//! Utilities for working with TermBufs in tests.
use crate::{
    core::termbuf::TermBuf,
    geom::Point,
    style::{Color, PartialStyle},
};

/// A helper macro to create buffers for the termbuf match assertions.
#[macro_export]
macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
}

/// A struct for configuring buffer matching behavior. By default, it treats 'X' as a special
/// marker for NULL cells in the buffer, allowing us to test partial renders.
pub struct BufTest<'a> {
    /// Reference to the buffer under test.
    buf: &'a TermBuf,
    /// Character used to represent NULL cells.
    null_char: char,
    /// Optional wildcard character.
    any_char: Option<char>,
}

impl<'a> BufTest<'a> {
    /// Create a new BufTest with a reference to a TermBuf.
    pub fn new(buf: &'a TermBuf) -> Self {
        Self {
            buf,
            null_char: 'X',
            any_char: None,
        }
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
    pub fn matches(&self, expected: &[&str]) -> bool {
        if expected.len() != self.buf.size().h as usize {
            return false;
        }

        for (y, expected_line) in expected.iter().enumerate() {
            // Get actual line character by character to handle NULL cells
            let mut actual_chars = Vec::new();
            for x in 0..self.buf.size().w {
                if let Some(cell) = self.buf.get(Point { x, y: y as u32 }) {
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
                if let Some(any) = self.any_char
                    && expected_ch == any
                {
                    continue; // any_char matches anything
                }
                if expected_ch != actual_ch {
                    return false;
                }
            }
        }

        true
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure.
    pub fn assert_matches(&self, expected: &[&str]) {
        self.assert_matches_with_context(expected, None);
    }

    /// Assert that the buffer matches the expected lines with pretty printed output on failure,
    /// with optional context information.
    pub fn assert_matches_with_context(&self, expected: &[&str], context: Option<&str>) {
        if !self.matches(expected) {
            let actual_lines = self.lines();
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

    /// Does the buffer contain the supplied substring?
    pub fn contains_text(&self, txt: &str) -> bool {
        self.lines().iter().any(|l| l.contains(txt))
    }

    /// Does the buffer contain the supplied substring in the given foreground colour?
    pub fn contains_text_fg(&self, txt: &str, fg: Color) -> bool {
        self.contains_text_style(txt, &PartialStyle::fg(fg))
    }

    /// Does the buffer contain the supplied substring with the given style?
    pub fn contains_text_style(&self, txt: &str, style: &PartialStyle) -> bool {
        let tl = txt.chars().count() as u32;
        if tl == 0 || tl > self.buf.size().w {
            return false;
        }
        for y in 0..self.buf.size().h {
            for x in 0..=self.buf.size().w.saturating_sub(tl) {
                let mut m = true;
                let mut c = false;
                for (i, ch) in txt.chars().enumerate() {
                    if let Some(cell) = self.buf.get(Point { x: x + i as u32, y }) {
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

    /// Dumps the contents of the buffer to the terminal for debugging purposes.
    pub fn dump(&self) {
        let width = self.buf.size().w as usize;

        println!(
            "\nTermBuf dump ({}x{}):",
            self.buf.size().w,
            self.buf.size().h
        );
        println!("┌{}┐", "─".repeat(width));

        for y in 0..self.buf.size().h {
            print!("│");
            for x in 0..self.buf.size().w {
                if let Some(cell) = self.buf.get(Point { x, y }) {
                    if cell.ch == '\0' {
                        print!("X");
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

    /// Dumps a single line from the buffer to the terminal for debugging purposes.
    pub fn dump_line(&self, line_num: u32) {
        if line_num >= self.buf.size().h {
            println!(
                "Error: Line {} is out of bounds (buffer height: {})",
                line_num,
                self.buf.size().h
            );
            return;
        }

        let width = self.buf.size().w as usize;

        println!(
            "\nTermBuf line {} (width: {}):",
            line_num,
            self.buf.size().w
        );
        println!("┌{}┐", "─".repeat(width));

        print!("│");
        for x in 0..self.buf.size().w {
            if let Some(cell) = self.buf.get(Point { x, y: line_num }) {
                if cell.ch == '\0' {
                    print!("X");
                } else {
                    print!("{}", cell.ch);
                }
            }
        }
        println!("│");

        println!("└{}┘", "─".repeat(width));

        // Bottom ruler
        print!(" ");
        for x in 0..width {
            print!("{}", x % 10);
        }
        println!();
    }

    /// Return the contents of a line as a `String`.
    pub fn line_text(&self, y: u32) -> Option<String> {
        if y >= self.buf.size().h {
            return None;
        }
        let mut ret = String::new();
        for x in 0..self.buf.size().w {
            if let Some(c) = self.buf.get(Point { x, y }) {
                ret.push(c.ch);
            }
        }
        Some(ret)
    }

    /// Return the contents of the buffer as lines of text.
    pub fn lines(&self) -> Vec<String> {
        (0..self.buf.size().h)
            .map(|y| {
                let mut chars = Vec::new();
                for x in 0..self.buf.size().w {
                    if let Some(cell) = self.buf.get(Point { x, y }) {
                        if cell.ch == '\0' {
                            chars.push('X');
                        } else {
                            chars.push(cell.ch);
                        }
                    }
                }
                chars.into_iter().collect()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::{Expanse, Line},
        style::{AttrSet, Color, Style},
    };

    fn test_style() -> Style {
        Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        }
    }

    #[test]
    fn test_bufmatch_default() {
        let mut buf = TermBuf::empty(Expanse::new(5, 3));
        buf.text(&test_style(), Line::new(0, 0, 5), "hello");

        let matcher = BufTest::new(&buf);
        assert!(matcher.matches(&["hello", "XXXXX", "XXXXX"]));
        assert!(!matcher.matches(&["world", "XXXXX", "XXXXX"]));
    }

    #[test]
    fn test_bufmatch_custom_null() {
        let mut buf = TermBuf::empty(Expanse::new(4, 2));
        buf.text(&test_style(), Line::new(0, 0, 2), "ab");

        let matcher = BufTest::new(&buf).with_null('_');
        assert!(matcher.matches(&["ab__", "____"]));
        assert!(!matcher.matches(&["abXX", "XXXX"])); // 'X' is not the null char anymore
    }

    #[test]
    fn test_bufmatch_any_char() {
        let mut buf = TermBuf::new(Expanse::new(4, 2), ' ', test_style());
        buf.text(&test_style(), Line::new(0, 0, 4), "test");
        buf.text(&test_style(), Line::new(0, 1, 4), "word");

        let matcher = BufTest::new(&buf).with_any('?');
        assert!(matcher.matches(&["????", "????"])); // all wildcards
        assert!(matcher.matches(&["te??", "wo??"])); // partial wildcards
        assert!(matcher.matches(&["test", "word"])); // exact match still works
        assert!(!matcher.matches(&["fail", "word"])); // wrong text
    }

    #[test]
    fn test_bufmatch_combined() {
        let mut buf = TermBuf::empty(Expanse::new(6, 2));
        buf.text(&test_style(), Line::new(0, 0, 3), "foo");

        let matcher = BufTest::new(&buf).with_null('_').with_any('*');
        assert!(matcher.matches(&["foo___", "______"])); // custom null char
        assert!(matcher.matches(&["***___", "******"])); // any + null
        assert!(matcher.matches(&["f**___", "______"])); // mixed
    }

    #[test]
    fn test_contains_functions() {
        let mut buf = TermBuf::new(Expanse::new(10, 2), ' ', test_style());

        let mut red_style = test_style();
        red_style.fg = Color::Red;

        buf.text(&test_style(), Line::new(0, 0, 5), "hello");
        buf.text(&red_style, Line::new(5, 0, 5), "world");

        let bt = BufTest::new(&buf);
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));

        assert!(bt.contains_text_fg("world", Color::Red));
        assert!(!bt.contains_text_fg("hello", Color::Red));

        assert!(bt.contains_text_style("world", &PartialStyle::fg(Color::Red)));
        assert!(bt.contains_text_style("hello", &PartialStyle::fg(Color::White)));
    }

    #[test]
    fn test_dump() {
        let mut buf = TermBuf::empty(Expanse::new(5, 3));
        buf.text(&test_style(), Line::new(0, 0, 5), "hello");
        buf.text(&test_style(), Line::new(1, 1, 3), "abc");

        // This test just verifies dump() runs without panicking
        // The actual output goes to stdout
        BufTest::new(&buf).dump();
    }

    #[test]
    fn test_dump_with_larger_buffer() {
        // Test with a larger buffer to see the ruler wrap around
        let mut buf = TermBuf::empty(Expanse::new(25, 15));
        buf.text(&test_style(), Line::new(0, 0, 10), "0123456789");
        buf.text(&test_style(), Line::new(10, 5, 15), "Offset at (10,5)");
        buf.text(&test_style(), Line::new(5, 10, 10), "Row 10 test");

        BufTest::new(&buf).dump();
    }

    #[test]
    fn test_dump_line() {
        let mut buf = TermBuf::empty(Expanse::new(20, 5));
        buf.text(&test_style(), Line::new(0, 0, 10), "First line");
        buf.text(&test_style(), Line::new(5, 2, 15), "Middle line at 5");
        buf.text(&test_style(), Line::new(0, 4, 20), "Last line with text!");

        // Test dumping various lines
        let bt = BufTest::new(&buf);
        bt.dump_line(0); // First line
        bt.dump_line(2); // Middle line
        bt.dump_line(4); // Last line
        bt.dump_line(10); // Out of bounds - should print error
    }

    #[test]
    fn test_buftest_instance_methods() {
        let mut buf = TermBuf::new(Expanse::new(10, 2), ' ', test_style());

        let mut red_style = test_style();
        red_style.fg = Color::Red;

        buf.text(&test_style(), Line::new(0, 0, 5), "hello");
        buf.text(&red_style, Line::new(5, 0, 5), "world");

        let bt = BufTest::new(&buf);

        // Test contains_text
        assert!(bt.contains_text("hello"));
        assert!(bt.contains_text("world"));
        assert!(!bt.contains_text("goodbye"));

        // Test contains_text_fg
        assert!(bt.contains_text_fg("world", Color::Red));
        assert!(!bt.contains_text_fg("hello", Color::Red));

        // Test contains_text_style
        assert!(bt.contains_text_style("world", &PartialStyle::fg(Color::Red)));
        assert!(bt.contains_text_style("hello", &PartialStyle::fg(Color::White)));

        // Test lines
        let lines = bt.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("helloworld"));

        // Test line_text
        assert_eq!(bt.line_text(0).unwrap().trim(), "helloworld");
    }
}
