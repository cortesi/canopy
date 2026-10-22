//! Terminal output: frame encoding and emission.

use std::{
    io::{self, Stderr, Write},
    iter,
};

use crossterm::{QueueableCommand, cursor as ccursor, style};

use super::translate_result;
use crate::{
    core::text,
    error::Result,
    geom::Point,
    render::RenderBackend,
    style::{Color, ResolvedStyle},
};

/// Append the decimal digits of `value`.
fn push_decimal(out: &mut Vec<u8>, value: u16) {
    let mut digits = [0; 5];
    let mut at = digits.len();
    let mut value = value;
    loop {
        at -= 1;
        digits[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[at..]);
}

/// Append the SGR parameters of a color after its `38;` or `48;`, as
/// crossterm writes them: the named colors as palette indices.
fn push_color(out: &mut Vec<u8>, color: Color) {
    let index = match color {
        Color::Rgb { r, g, b } => {
            out.extend_from_slice(b"2");
            for channel in [r, g, b] {
                out.push(b';');
                push_decimal(out, channel.into());
            }
            return;
        }
        Color::AnsiValue(value) => value,
        Color::Black => 0,
        Color::DarkRed => 1,
        Color::DarkGreen => 2,
        Color::DarkYellow => 3,
        Color::DarkBlue => 4,
        Color::DarkMagenta => 5,
        Color::DarkCyan => 6,
        Color::Grey => 7,
        Color::DarkGrey => 8,
        Color::Red => 9,
        Color::Green => 10,
        Color::Yellow => 11,
        Color::Blue => 12,
        Color::Magenta => 13,
        Color::Cyan => 14,
        Color::White => 15,
    };
    out.extend_from_slice(b"5;");
    push_decimal(out, index.into());
}

/// Append a cursor move to `(x, y)`.
fn push_move(out: &mut Vec<u8>, x: u16, y: u16) {
    out.extend_from_slice(b"\x1b[");
    push_decimal(out, y.saturating_add(1));
    out.push(b';');
    push_decimal(out, x.saturating_add(1));
    out.push(b'H');
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
    /// Where the encoded commands leave the cursor, when that is known.
    cursor: Option<(u16, u16)>,
    /// The style the encoded commands set last this frame.
    style: Option<ResolvedStyle>,
}

impl CrosstermRender {
    /// Write APC strings, `ESC _ payload ESC \`, after the emitted frame.
    pub(super) fn write_apcs(&mut self, payloads: &[String]) -> io::Result<()> {
        for payload in payloads {
            encode_apc(&mut self.pending, payload);
        }
        self.flush()
    }

    /// Flush pending output.
    fn flush(&mut self) -> io::Result<()> {
        flush_frame(&mut self.fp.lock(), &mut self.pending)
    }

    /// Apply a style to subsequent output, in one SGR sequence that sets
    /// what differs from the style this frame set last. A change of
    /// attributes resets them all, and then both colors.
    fn apply_style(&mut self, s: &ResolvedStyle) {
        let last = self.style.replace(*s);
        if last == Some(*s) {
            return;
        }
        let kept = last.filter(|last| last.attrs == s.attrs);
        let out = &mut self.pending;
        out.push(0x1b);
        // Each parameter follows a `;`, and the first becomes the `[`.
        let start = out.len();
        if kept.is_none() {
            out.extend_from_slice(b";0");
            let attrs = s.attrs;
            for (on, code) in [
                (attrs.bold, b";1".as_slice()),
                (attrs.crossedout, b";9"),
                (attrs.dim, b";2"),
                (attrs.italic, b";3"),
                (attrs.overline, b";53"),
                (attrs.underline, b";4"),
            ] {
                if on {
                    out.extend_from_slice(code);
                }
            }
        }
        // Crossterm leaves colors out under NO_COLOR, and so does this.
        if !style::Colored::ansi_color_disabled_memoized() {
            if kept.is_none_or(|kept| kept.fg != s.fg) {
                out.extend_from_slice(b";38;");
                push_color(out, s.fg);
            }
            if kept.is_none_or(|kept| kept.bg != s.bg) {
                out.extend_from_slice(b";48;");
                push_color(out, s.bg);
            }
        }
        if out.len() == start {
            out.pop();
            return;
        }
        out[start] = b'[';
        out.push(b'm');
    }

    /// Write text at a position.
    fn text(&mut self, loc: Point, txt: &str) -> io::Result<()> {
        for run in positioned_text_runs(loc, txt) {
            let x = cell_coord(run.location.x)?;
            let y = cell_coord(run.location.y)?;
            if self.cursor != Some((x, y)) {
                push_move(&mut self.pending, x, y);
            }
            self.pending.extend_from_slice(run.text.as_bytes());
            // Every terminal agrees on the width of printable ASCII, so the
            // cursor's place after it is known.
            self.cursor = text::printable_ascii(run.text)
                .then(|| u16::try_from(run.text.len()).ok())
                .flatten()
                .and_then(|columns| x.checked_add(columns))
                .map(|end| (end, y));
        }
        Ok(())
    }
}

/// Append the APC string `ESC _ payload ESC \` to `out`.
fn encode_apc(out: &mut Vec<u8>, payload: &str) {
    out.extend_from_slice(b"\x1b_");
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(b"\x1b\\");
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
    let mut graphemes = text::graphemes(txt)
        .map(|grapheme| (grapheme.as_ptr() as usize - txt.as_ptr() as usize, grapheme))
        .peekable();
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
            cursor: None,
            style: None,
        }
    }
}

impl RenderBackend for CrosstermRender {
    fn reset(&mut self) -> Result<()> {
        self.pending.clear();
        self.cursor = None;
        self.style = None;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        translate_result(self.flush())
    }

    fn style(&mut self, s: &ResolvedStyle) -> Result<()> {
        self.apply_style(s);
        Ok(())
    }

    fn text(&mut self, loc: Point, txt: &str) -> Result<()> {
        translate_result(self.text(loc, txt))
    }

    fn park_cursor(&mut self, location: Option<Point>) -> Result<()> {
        let Some(location) = location else {
            return Ok(());
        };
        self.cursor = None;
        translate_result((|| {
            let x = cell_coord(location.x)?;
            let y = cell_coord(location.y)?;
            self.pending.queue(ccursor::MoveTo(x, y)).map(|_| ())
        })())
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

        self.cursor = None;
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
        // Resetting the scroll region moves the cursor.
        self.cursor = None;
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

    #[test]
    fn an_apc_string_wraps_its_payload() {
        let mut out = Vec::new();
        encode_apc(&mut out, "itty:mark:7");
        assert_eq!(out, b"\x1b_itty:mark:7\x1b\\");
    }

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
    fn a_style_sets_only_what_changed_since_the_last() -> Result<()> {
        use crate::style::{Attr, AttrSet};

        let plain = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let red = ResolvedStyle {
            fg: Color::Rgb { r: 255, g: 0, b: 9 },
            ..plain
        };
        let bold = ResolvedStyle {
            attrs: AttrSet::new(Attr::Bold).with(Attr::Underline),
            ..red
        };
        let mut backend = CrosstermRender::default();
        for style in [plain, red, red, bold, plain] {
            backend.style(&style)?;
        }
        assert_eq!(
            String::from_utf8_lossy(&backend.pending),
            concat!(
                "\x1b[0;38;5;15;48;5;0m",
                "\x1b[38;2;255;0;9m",
                "\x1b[0;1;4;38;2;255;0;9;48;5;0m",
                "\x1b[0;38;5;15;48;5;0m",
            )
        );
        backend.reset()?;
        backend.style(&red)?;
        assert_eq!(
            String::from_utf8_lossy(&backend.pending),
            "\x1b[0;38;2;255;0;9;48;5;0m",
            "a frame starts with no style set"
        );
        Ok(())
    }

    #[test]
    fn text_after_ascii_on_its_row_moves_no_cursor() -> Result<()> {
        let mut backend = CrosstermRender::default();
        backend.text(Point { x: 1, y: 0 }, "ab").unwrap();
        backend.text(Point { x: 3, y: 0 }, "cd").unwrap();
        // A wide glyph leaves the cursor where the terminal puts it.
        backend.text(Point { x: 5, y: 0 }, "界").unwrap();
        backend.text(Point { x: 7, y: 0 }, "e").unwrap();
        assert_eq!(
            String::from_utf8_lossy(&backend.pending),
            "\x1b[1;2Habcd界\x1b[1;8He"
        );
        backend.reset()?;
        backend.text(Point { x: 8, y: 0 }, "f").unwrap();
        assert_eq!(
            String::from_utf8_lossy(&backend.pending),
            "\x1b[1;9Hf",
            "a frame starts with the cursor unknown"
        );
        Ok(())
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
