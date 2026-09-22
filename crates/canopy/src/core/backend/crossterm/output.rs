//! Terminal output: frame encoding and emission.

use std::{
    io::{self, Stderr, Write},
    iter,
};

use crossterm::{QueueableCommand, cursor as ccursor, style};
use unicode_segmentation::UnicodeSegmentation;

use super::translate_result;
use crate::{
    core::text,
    error::Result,
    geom::Point,
    render::RenderBackend,
    style::{Color, ResolvedStyle},
};

/// Translate a canopy color into a crossterm color.
fn translate_color(c: Color) -> style::Color {
    match c {
        Color::Black => style::Color::Black,
        Color::DarkGrey => style::Color::DarkGrey,
        Color::Red => style::Color::Red,
        Color::DarkRed => style::Color::DarkRed,
        Color::Green => style::Color::Green,
        Color::DarkGreen => style::Color::DarkGreen,
        Color::Yellow => style::Color::Yellow,
        Color::DarkYellow => style::Color::DarkYellow,
        Color::Blue => style::Color::Blue,
        Color::DarkBlue => style::Color::DarkBlue,
        Color::Magenta => style::Color::Magenta,
        Color::DarkMagenta => style::Color::DarkMagenta,
        Color::Cyan => style::Color::Cyan,
        Color::DarkCyan => style::Color::DarkCyan,
        Color::White => style::Color::White,
        Color::Grey => style::Color::Grey,
        Color::Rgb { r, g, b } => style::Color::Rgb { r, g, b },
        Color::AnsiValue(a) => style::Color::AnsiValue(a),
    }
}

/// Convert a terminal cell coordinate into the crossterm `u16` range.
fn cell_coord(value: u32) -> io::Result<u16> {
    u16::try_from(value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal coordinate exceeds u16",
        )
    })
}

/// Crossterm-backed render backend.
pub struct CrosstermRender {
    /// Stderr handle used for rendering output.
    fp: Stderr,
    /// Encoded commands for the current frame, discarded if emission fails.
    pending: Vec<u8>,
}

impl CrosstermRender {
    /// Flush pending output.
    fn flush(&mut self) -> io::Result<()> {
        flush_frame(&mut self.fp.lock(), &mut self.pending)
    }

    /// Apply a style to subsequent output.
    fn apply_style(&mut self, s: &ResolvedStyle) -> io::Result<()> {
        // Always reset first to clear any previous attributes, then set colors
        // and attrs. Order is important: reset clears everything, so we
        // must set colors after.
        self.pending
            .queue(style::SetAttribute(style::Attribute::Reset))?;
        self.pending
            .queue(style::SetForegroundColor(translate_color(s.fg)))?;
        self.pending
            .queue(style::SetBackgroundColor(translate_color(s.bg)))?;

        // Now add the desired attributes
        if s.attrs.bold {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Bold))?;
        }
        if s.attrs.crossedout {
            self.pending
                .queue(style::SetAttribute(style::Attribute::CrossedOut))?;
        }
        if s.attrs.dim {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Dim))?;
        }
        if s.attrs.italic {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Italic))?;
        }
        if s.attrs.overline {
            self.pending
                .queue(style::SetAttribute(style::Attribute::OverLined))?;
        }
        if s.attrs.underline {
            self.pending
                .queue(style::SetAttribute(style::Attribute::Underlined))?;
        }
        Ok(())
    }

    /// Write text at a position.
    fn text(&mut self, loc: Point, txt: &str) -> io::Result<()> {
        for run in positioned_text_runs(loc, txt) {
            let x = cell_coord(run.location.x)?;
            let y = cell_coord(run.location.y)?;
            self.pending.queue(ccursor::MoveTo(x, y))?;
            self.pending.queue(style::Print(run.text))?;
        }
        Ok(())
    }
}

/// Write one frame and discard its bytes even on a partial write or flush
/// error. The caller can then encode a full repaint for the next attempt.
fn flush_frame(writer: &mut impl Write, pending: &mut Vec<u8>) -> io::Result<()> {
    let result = writer.write_all(pending);
    pending.clear();
    result?;
    writer.flush()
}

/// A string fragment with an absolute terminal-cell location.
#[derive(Debug, PartialEq, Eq)]
struct PositionedTextRun<'a> {
    /// Location where the text run should be printed.
    location: Point,
    /// Text to print at the location.
    text: &'a str,
}

/// Borrow absolute-positioned runs, isolating wide graphemes and omitting
/// zero-width graphemes without allocating intermediate strings.
fn positioned_text_runs(loc: Point, txt: &str) -> impl Iterator<Item = PositionedTextRun<'_>> {
    let mut graphemes = txt.grapheme_indices(true).peekable();
    let mut x = loc.x;

    iter::from_fn(move || {
        loop {
            let (start, grapheme) = graphemes.next()?;
            let width = text::grapheme_width(grapheme);
            if width == 0 {
                continue;
            }

            let location = Point { x, y: loc.y };
            let mut end = start + grapheme.len();
            x = x.saturating_add(width as u32);
            if width == 1 {
                while let Some((offset, following)) =
                    graphemes.next_if(|(_, next)| text::grapheme_width(next) == 1)
                {
                    end = offset + following.len();
                    x = x.saturating_add(1);
                }
            }
            return Some(PositionedTextRun {
                location,
                text: &txt[start..end],
            });
        }
    })
}

impl Default for CrosstermRender {
    fn default() -> Self {
        Self {
            fp: io::stderr(),
            pending: Vec::with_capacity(64 * 1024),
        }
    }
}

impl RenderBackend for CrosstermRender {
    fn reset(&mut self) -> Result<()> {
        self.pending.clear();
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        translate_result(self.flush())
    }

    fn style(&mut self, s: &ResolvedStyle) -> Result<()> {
        translate_result(self.apply_style(s))
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        translate_result(self.text(loc, txt))
    }

    fn supports_char_shift(&self) -> bool {
        true
    }

    fn supports_line_shift(&self) -> bool {
        true
    }

    fn shift_chars(&mut self, loc: Point, count: i32) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        let x = translate_result(cell_coord(loc.x))?;
        let y = translate_result(cell_coord(loc.y))?;
        translate_result(self.pending.queue(ccursor::MoveTo(x, y)))?;
        let op = if count > 0 { '@' } else { 'P' };
        translate_result(write!(self.pending, "\x1b[{count_abs}{op}"))
    }

    fn shift_lines(&mut self, top: u32, bottom: u32, count: i32) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let top = top.min(u16::MAX as u32) as u16;
        let bottom = bottom.min(u16::MAX as u32) as u16;
        if top > bottom {
            return Ok(());
        }
        let count_abs = count.unsigned_abs().min(u16::MAX as u32) as u16;
        translate_result(write!(self.pending, "\x1b[{};{}r", top + 1, bottom + 1))?;
        translate_result(self.pending.queue(ccursor::MoveTo(0, top)))?;
        let op = if count > 0 { 'T' } else { 'S' };
        translate_result(write!(self.pending, "\x1b[{count_abs}{op}\x1b[r"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FrameCapture {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
        fail_after: Option<usize>,
        fail_flush: bool,
    }

    impl Write for FrameCapture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            let len = self.fail_after.map_or(bytes.len(), |limit| {
                limit.saturating_sub(self.bytes.len()).min(bytes.len())
            });
            if len == 0 && !bytes.is_empty() {
                return Err(io::Error::other("injected frame write failure"));
            }
            self.bytes.extend_from_slice(&bytes[..len]);
            Ok(len)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(io::Error::other("injected frame flush failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn terminal_frame_batches_text_and_shifts_until_flush() -> Result<()> {
        let mut backend = CrosstermRender::default();
        backend.text(Point { x: 1, y: 2 }, "hi").unwrap();
        backend.shift_chars(Point { x: 3, y: 2 }, 2)?;
        backend.shift_lines(2, 4, -1)?;
        let expected = b"\x1b[3;2Hhi\x1b[3;4H\x1b[2@\x1b[3;5r\x1b[3;1H\x1b[1S\x1b[r";
        assert_eq!(backend.pending, expected);

        let mut capture = FrameCapture::default();
        flush_frame(&mut capture, &mut backend.pending).unwrap();
        assert_eq!(capture.bytes, expected);
        assert_eq!((capture.writes, capture.flushes), (1, 1));
        assert!(backend.pending.is_empty());

        backend.text(Point::default(), "discarded").unwrap();
        backend.reset()?;
        assert!(backend.pending.is_empty());
        Ok(())
    }

    #[test]
    fn terminal_frame_failure_discards_bytes_before_a_fresh_retry() {
        for fail_after in [None, Some(3)] {
            let mut capture = FrameCapture {
                fail_after,
                fail_flush: fail_after.is_none(),
                ..FrameCapture::default()
            };
            let mut pending = b"first frame".to_vec();
            assert!(flush_frame(&mut capture, &mut pending).is_err());
            assert!(
                pending.is_empty(),
                "failed output must not survive for drop or retry"
            );
            assert_eq!(
                capture.bytes,
                if fail_after.is_some() {
                    &b"fir"[..]
                } else {
                    &b"first frame"[..]
                }
            );

            capture.bytes.clear();
            capture.fail_after = None;
            capture.fail_flush = false;
            pending.extend_from_slice(b"fresh frame");
            flush_frame(&mut capture, &mut pending).unwrap();
            assert_eq!(capture.bytes, b"fresh frame");
        }
    }

    fn text_run(x: u32, y: u32, text: &str) -> PositionedTextRun<'_> {
        PositionedTextRun {
            location: Point { x, y },
            text,
        }
    }

    #[test]
    fn positioned_text_runs_split_after_wide_graphemes() {
        let runs: Vec<_> = positioned_text_runs(Point { x: 5, y: 2 }, "a界bc").collect();

        assert_eq!(
            runs,
            vec![
                text_run(5, 2, "a"),
                text_run(6, 2, "界"),
                text_run(8, 2, "bc"),
            ]
        );
    }

    #[test]
    fn positioned_text_runs_keep_combining_graphemes_in_run() {
        let runs: Vec<_> = positioned_text_runs(Point { x: 1, y: 3 }, "e\u{0301}x").collect();

        assert_eq!(runs, vec![text_run(1, 3, "e\u{0301}x")]);
    }

    #[test]
    fn positioned_text_runs_skip_zero_width_without_losing_columns() {
        let runs: Vec<_> = positioned_text_runs(
            Point { x: 0, y: 2 },
            "\u{200b}a\u{200b}b\u{0301}界\u{200b}x",
        )
        .collect();
        assert_eq!(
            runs,
            [
                text_run(0, 2, "a"),
                text_run(1, 2, "b\u{0301}"),
                text_run(2, 2, "界"),
                text_run(4, 2, "x"),
            ]
        );
        assert!(
            positioned_text_runs(Point::default(), "\u{200b}")
                .next()
                .is_none()
        );
    }

    #[test]
    fn positioned_text_runs_saturate_coordinates() {
        let runs: Vec<_> = positioned_text_runs(
            Point {
                x: u32::MAX - 1,
                y: 0,
            },
            "界x",
        )
        .collect();
        assert_eq!(
            runs,
            [text_run(u32::MAX - 1, 0, "界"), text_run(u32::MAX, 0, "x")]
        );
    }
}
