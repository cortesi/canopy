use crate::termbuf::TermBuf;

/// A helper macro to create buffers for the termbuf match assertions.
#[macro_export]
macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
}

/// Asserts that the buffer contents match the expected lines of text.
///
/// This function compares the characters in the buffer against an expected
/// set of lines, ignoring all styling information. It's useful for testing
/// rendering output where only the text content matters.
///
/// # Arguments
///
/// * `expected` - A vector of strings representing the expected lines.
///   Each string should match one line of the buffer.
///
/// # Panics
///
/// Panics if:
/// - The number of lines doesn't match
/// - Any line content doesn't match the expected text
///
/// # Example
///
/// ```no_run
/// # use canopy::{TermBuf, geom::Expanse, style::Style};
/// # let buf = TermBuf::new(Expanse::new(5, 3), ' ', Style::default());
/// buf.assert_buffer_matches(&[
///     "Hello",
///     "World",
///     "     ",
/// ]);
/// ```
pub fn assert_buffer_matches(buf: &TermBuf, expected: &[&str]) {
    let actual_lines = buf.lines();

    assert_eq!(
        expected.len(),
        buf.size().h as usize,
        "Expected {} lines, but buffer has {} lines",
        expected.len(),
        buf.size().h
    );

    for (y, expected_line) in expected.iter().enumerate() {
        let actual_line = &actual_lines[y];
        assert_eq!(
            actual_line.trim_end(),
            expected_line.trim_end(),
            "Line {y} mismatch:\nExpected: '{expected_line}'\nActual:   '{actual_line}'"
        );
    }
}

/// Returns true if the buffer content matches the expected lines.
/// This is the non-panicking version of assert_buffer_matches.
///
/// In expected lines, the character 'X' is treated as a special marker for NULL cells.
/// NULL cells (containing '\0') in the actual buffer will match 'X' in the expected pattern.
pub fn buffer_matches(buf: &TermBuf, expected: &[&str]) -> bool {
    if expected.len() != buf.size().h as usize {
        return false;
    }

    for (y, expected_line) in expected.iter().enumerate() {
        // Get actual line character by character to handle NULL cells
        let mut actual_chars = Vec::new();
        for x in 0..buf.size().w {
            if let Some(cell) = buf.get(crate::geom::Point { x, y: y as u32 }) {
                if cell.ch == '\0' {
                    actual_chars.push('X');
                } else {
                    actual_chars.push(cell.ch);
                }
            }
        }
        let actual_line: String = actual_chars.into_iter().collect();

        if actual_line.trim_end() != expected_line.trim_end() {
            return false;
        }
    }

    true
}

/// Assert that the buffer matches the expected lines with pretty printed output on failure.
///
/// In expected lines, the character 'X' is treated as a special marker for NULL cells.
/// NULL cells (containing '\0') in the actual buffer will match 'X' in the expected pattern.
/// This is useful for testing partial renders where some areas remain uninitialized.
///
/// # Example
/// ```ignore
/// buffer.assert_matches(&[
///     "Hello X",  // 'X' matches NULL cells
///     "World  ",
/// ]);
/// ```
pub fn assert_matches(buf: &TermBuf, expected: &[&str]) {
    assert_matches_with_context(buf, expected, None);
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
    if !buffer_matches(buf, expected) {
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
