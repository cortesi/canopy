use canopy::{geom::Point, text};
use icu_segmenter::{LineSegmenter, LineSegmenterBorrowed, options::LineBreakOptions};
use unicode_segmentation::UnicodeSegmentation;

use super::WrapMode;
use crate::text_buffer::{LineChange, TextBuffer, TextPosition};

/// How a layout breaks logical lines into display rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wrap {
    /// Wrapping mode.
    pub mode: WrapMode,
    /// Columns that text fills before a soft-wrapped row breaks.
    pub width: usize,
    /// Tab stop width in columns.
    pub tab_stop: usize,
    /// Whether the editor shows a caret. A line whose last row ends in
    /// blanks past the wrap width then gets an empty row, where the caret
    /// after them shows.
    pub caret: bool,
}

impl Wrap {
    /// Construct wrap parameters. The width is at least one column.
    pub fn new(mode: WrapMode, width: usize, tab_stop: usize, caret: bool) -> Self {
        Self {
            mode,
            width: width.max(1),
            tab_stop,
            caret,
        }
    }

    /// Return the widest caret column of a row. Soft wrapping limits it to
    /// the wrap width: blanks that hang past it share that column.
    fn caret_limit(self) -> usize {
        match self.mode {
            WrapMode::None => usize::MAX,
            WrapMode::Soft => self.width,
        }
    }
}

/// A wrapped segment of a logical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapSegment {
    /// Starting char index of this segment.
    pub start_char: usize,
    /// Ending char index of this segment.
    pub end_char: usize,
    /// Display column where this segment starts.
    pub start_col: usize,
    /// Display column where this segment ends.
    pub end_col: usize,
}

impl WrapSegment {
    /// Construct the segment between two marks of a line.
    fn between(start: Mark, end: Mark) -> Self {
        Self {
            start_char: start.char,
            end_char: end.char,
            start_col: start.col,
            end_col: end.col,
        }
    }

    /// Return the display width of this segment.
    pub fn width(&self) -> usize {
        self.end_col.saturating_sub(self.start_col)
    }
}

/// Layout information for a single logical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineLayout {
    /// Wrapped segments that make up the line.
    pub segments: Vec<WrapSegment>,
    /// Total display width of the line.
    pub display_width: usize,
}

impl LineLayout {
    /// Return the number of display lines for this logical line.
    pub fn display_lines(&self) -> usize {
        self.segments.len().max(1)
    }

    /// Return the segment for the provided index.
    pub fn segment(&self, idx: usize) -> Option<&WrapSegment> {
        self.segments.get(idx)
    }

    /// Find the segment index containing a display column.
    pub fn segment_for_column(&self, column: usize) -> usize {
        for (idx, seg) in self.segments.iter().enumerate() {
            if column < seg.end_col {
                return idx;
            }
        }
        self.segments.len().saturating_sub(1)
    }
}

/// Layout cache for mapping text positions to display coordinates.
#[derive(Debug, Clone)]
pub struct LayoutCache {
    /// Cached line layouts for the current buffer.
    lines: Vec<LineLayout>,
    /// Prefix offsets for display lines.
    line_offsets: Vec<usize>,
    /// Total display line count.
    total_lines: usize,
    /// Maximum display width across all lines.
    max_line_width: usize,
    /// Cached wrap parameters.
    wrap: Wrap,
    /// Cached buffer revision.
    revision: u64,
}

impl LayoutCache {
    /// Construct a new layout cache.
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            line_offsets: vec![0],
            total_lines: 0,
            max_line_width: 0,
            // No wrap parameters have a width of zero, so the first sync
            // rebuilds.
            wrap: Wrap {
                mode: WrapMode::None,
                width: 0,
                tab_stop: 4,
                caret: false,
            },
            revision: 0,
        }
    }

    /// Synchronize layout state with the buffer and wrapping parameters.
    pub fn sync(&mut self, buffer: &mut TextBuffer, wrap: Wrap) {
        if self.wrap != wrap {
            let _ = buffer.take_change();
            self.rebuild_all(buffer, wrap);
            return;
        }

        let revision = buffer.revision();
        if revision == self.revision {
            return;
        }

        if let Some(change) = buffer.take_change() {
            self.apply_change(buffer, change);
        } else {
            self.rebuild_all(buffer, wrap);
        }

        self.revision = revision;
    }

    /// Return the cached `(total_lines, max_line_width)` when the cache is
    /// current for the buffer and the wrap parameters. A stale cache returns
    /// `None` so the caller can fall back to a direct scan.
    pub fn metrics_for(&self, buffer: &TextBuffer, wrap: Wrap) -> Option<(usize, usize)> {
        (self.wrap == wrap && self.revision == buffer.revision())
            .then_some((self.total_lines, self.max_line_width))
    }

    /// Return the cached width of the widest line when the cache is current
    /// for the buffer and the tab stop. Line widths do not depend on
    /// wrapping.
    pub fn widest_for(&self, buffer: &TextBuffer, tab_stop: usize) -> Option<usize> {
        (self.wrap.width > 0
            && self.wrap.tab_stop == tab_stop
            && self.revision == buffer.revision())
        .then_some(self.max_line_width)
    }

    /// Return the total number of display lines.
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Return the line layout for an index.
    pub fn line(&self, index: usize) -> Option<&LineLayout> {
        self.lines.get(index)
    }

    /// Return the display line offset for a logical line.
    pub fn line_offset(&self, index: usize) -> usize {
        self.line_offsets
            .get(index)
            .copied()
            .unwrap_or(self.total_lines)
    }

    /// Map a text position to display coordinates.
    pub fn point_for_position(&self, buffer: &TextBuffer, position: TextPosition) -> Point {
        let line = position.line.min(self.lines.len().saturating_sub(1));
        let Some(layout) = self.lines.get(line) else {
            return Point { x: 0, y: 0 };
        };
        let display_col = buffer.column_for_position(position, self.wrap.tab_stop);
        segment_point(
            layout,
            display_col,
            self.line_offset(line),
            self.wrap.caret_limit(),
        )
    }

    /// Map a column of a display row to the closest text position on that
    /// row. The column clamps to the row: to its end on the last row of a
    /// line, and to its last grapheme on the other rows, whose end shows at
    /// the start of the next row. On a soft-wrapped row, it also clamps to
    /// the wrap width, past which hanging blanks do not show.
    pub fn position_in_row(&self, buffer: &TextBuffer, row: usize, column: usize) -> TextPosition {
        let line = self.line_for_display(row);
        let Some(layout) = self.lines.get(line) else {
            return TextPosition::new(0, 0);
        };
        let index = row
            .saturating_sub(self.line_offset(line))
            .min(layout.segments.len().saturating_sub(1));
        let Some(segment) = layout.segment(index) else {
            return TextPosition::new(line, 0);
        };
        let widest = if index + 1 == layout.segments.len() {
            segment.width()
        } else {
            segment.width().saturating_sub(1)
        }
        .min(self.wrap.caret_limit());
        buffer.position_for_column(
            line,
            segment.start_col.saturating_add(column.min(widest)),
            self.wrap.tab_stop,
        )
    }

    /// Return the logical line index for a display line.
    pub(crate) fn line_for_display(&self, y: usize) -> usize {
        if self.line_offsets.len() <= 1 {
            return 0;
        }
        let mut low = 0usize;
        let mut high = self.line_offsets.len().saturating_sub(1);
        while low < high {
            let mid = (low + high).div_ceil(2);
            if self.line_offsets[mid] <= y {
                low = mid;
            } else {
                high = mid.saturating_sub(1);
            }
        }
        low.min(self.lines.len().saturating_sub(1))
    }

    /// Rebuild the entire layout cache.
    fn rebuild_all(&mut self, buffer: &TextBuffer, wrap: Wrap) {
        self.lines.clear();
        let line_count = buffer.line_count().max(1);
        for line in 0..line_count {
            self.lines.push(layout_line(&buffer.line_text(line), wrap));
        }
        self.rebuild_offsets();
        self.wrap = wrap;
        self.revision = buffer.revision();
    }

    /// Apply an incremental line change to the layout cache.
    fn apply_change(&mut self, buffer: &TextBuffer, change: LineChange) {
        let start = change.start_line.min(self.lines.len());
        let end = start
            .saturating_add(change.old_line_count)
            .min(self.lines.len());
        let new_end = start.saturating_add(change.new_line_count);
        let replacement: Vec<_> = (start..new_end)
            .map(|line| layout_line(&buffer.line_text(line), self.wrap))
            .collect();
        self.lines.splice(start..end, replacement);
        if self.lines.is_empty() {
            self.lines.push(layout_line("", self.wrap));
        }
        self.rebuild_offsets();
    }

    /// Rebuild prefix offsets and aggregate metrics.
    fn rebuild_offsets(&mut self) {
        self.line_offsets.clear();
        self.line_offsets.push(0);
        let mut total = 0usize;
        let mut max_width = 0usize;
        for line in &self.lines {
            let line_count = line.display_lines();
            total = total.saturating_add(line_count);
            self.line_offsets.push(total);
            max_width = max_width.max(line.display_width);
        }
        self.total_lines = total.max(1);
        self.max_line_width = max_width.max(1);
    }
}

/// Map a text position to display coordinates by laying out the lines up to
/// it, without a cache.
pub fn point_for_position(buffer: &TextBuffer, position: TextPosition, wrap: Wrap) -> Point {
    let line = position.line.min(buffer.line_count().max(1) - 1);
    let layout_of = |index| layout_line(&buffer.line_text(index), wrap);
    let row = match wrap.mode {
        WrapMode::None => line,
        WrapMode::Soft => (0..line)
            .map(|index| layout_of(index).display_lines())
            .sum(),
    };
    let display_col = buffer.column_for_position(position, wrap.tab_stop);
    segment_point(&layout_of(line), display_col, row, wrap.caret_limit())
}

/// Return the display point of a column in a line whose first segment starts
/// at display row `row`, with the column clamped to `limit`.
fn segment_point(layout: &LineLayout, display_col: usize, row: usize, limit: usize) -> Point {
    let index = layout.segment_for_column(display_col);
    let start = layout.segment(index).map_or(0, |segment| segment.start_col);
    Point {
        x: display_col.saturating_sub(start).min(limit) as u32,
        y: row.saturating_add(index) as u32,
    }
}

/// Compute `(display_line_count, max_line_width)` by scanning the buffer.
///
/// `display_width` is independent of wrapping, so one pass supplies both.
pub fn metrics(buffer: &TextBuffer, wrap: Wrap) -> (usize, usize) {
    let mut lines = 0usize;
    let mut max_width = 1usize;
    for line in 0..buffer.line_count().max(1) {
        let layout = layout_line(&buffer.line_text(line), wrap);
        lines = lines.saturating_add(layout.display_lines());
        max_width = max_width.max(layout.display_width);
    }
    (lines.max(1), max_width)
}

/// The line break opportunities of the Unicode line breaking algorithm
/// (UAX #14). Scripts that need dictionary or model segmentation to find
/// words, such as Thai, have no opportunities inside a run.
const LINE_BREAKS: LineSegmenterBorrowed<'static> =
    LineSegmenter::new_for_non_complex_scripts(LineBreakOptions::default());

/// A position in a line: a char index and its display column.
#[derive(Debug, Clone, Copy)]
struct Mark {
    /// Char index in the line.
    char: usize,
    /// Display column of the char.
    col: usize,
}

/// Return whether a grapheme is a blank, which can hang past the wrap width.
fn is_blank(grapheme: &str) -> bool {
    matches!(grapheme, " " | "\t")
}

/// Build layout segments for a single logical line.
///
/// Soft wrapping fills each row with graphemes. When a grapheme does not
/// fit, the row ends at its last line break opportunity after a non-blank
/// grapheme, or else before the grapheme, which breaks a word wider than
/// the row. Blanks after a non-blank grapheme never end a row: they stay at
/// its end, past the wrap width if need be. When they take the last row of
/// the line past the wrap width in an editor that shows a caret, an empty
/// row follows, where the caret after them shows.
pub fn layout_line(text: &str, wrap: Wrap) -> LineLayout {
    let wrap_width = wrap.width.max(1);
    // A grapheme takes no more columns than bytes, except a tab, so a short
    // line without tabs fits its row and needs no breaks.
    let fits = text.len() <= wrap_width && !text.contains('\t');
    let mut breaks =
        (wrap.mode == WrapMode::Soft && !fits).then(|| LINE_BREAKS.segment_str(text).peekable());
    let mut segments = Vec::new();
    let mut next = Mark { char: 0, col: 0 };
    let mut start = next;
    // Whether the row has a non-blank grapheme.
    let mut content = false;
    // The last break of the row after a non-blank grapheme, and whether a
    // non-blank grapheme follows it.
    let mut last_break: Option<Mark> = None;
    let mut content_after_break = false;
    // Whether the last grapheme hangs past the end of its row.
    let mut hanging = false;

    for (byte, grapheme) in text.grapheme_indices(true) {
        let width = text::cell_width(grapheme, next.col, wrap.tab_stop);
        let blank = is_blank(grapheme);
        if let Some(breaks) = breaks.as_mut() {
            // An opportunity inside a grapheme is not a break.
            while breaks.next_if(|&offset| offset < byte).is_some() {}
            if breaks.next_if_eq(&byte).is_some() && content {
                last_break = Some(next);
                content_after_break = false;
            }
            let overflows =
                |start: Mark| next.col > start.col && next.col - start.col + width > wrap_width;
            hanging = blank && content;
            if !hanging && overflows(start) {
                if let Some(at) = last_break.take() {
                    segments.push(WrapSegment::between(start, at));
                    start = at;
                    content = content_after_break;
                }
                // The graphemes after the break and this one can still be
                // too wide, as with a wide grapheme in a narrow row.
                if overflows(start) {
                    segments.push(WrapSegment::between(start, next));
                    start = next;
                    content = false;
                }
            }
        }
        if !blank {
            content = true;
            content_after_break = true;
        }
        next.col = next.col.saturating_add(width);
        next.char = next.char.saturating_add(grapheme.chars().count());
    }

    segments.push(WrapSegment::between(start, next));
    if wrap.caret && hanging && next.col - start.col > wrap_width {
        segments.push(WrapSegment::between(next, next));
    }
    LineLayout {
        segments,
        display_width: next.col,
    }
}

#[cfg(test)]
mod tests {
    use std::iter;

    use proptest::prelude::*;

    use super::*;
    use crate::text_buffer::TextRange;

    /// Return soft wrap parameters for an editor that shows a caret.
    fn soft(width: usize) -> Wrap {
        Wrap::new(WrapMode::Soft, width, 4, true)
    }

    /// Return the text of each soft-wrapped row of a line.
    fn rows(text: &str, wrap_width: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        layout_line(text, soft(wrap_width))
            .segments
            .iter()
            .map(|segment| chars[segment.start_char..segment.end_char].iter().collect())
            .collect()
    }

    #[test]
    fn wrap_layout_splits_lines() {
        let line = "hello";
        let layout = layout_line(line, soft(2));
        assert_eq!(layout.segments.len(), 3);
        assert_eq!(layout.display_width, 5);
    }

    #[test]
    fn soft_wrap_breaks_between_words() {
        assert_eq!(rows("the quick brown fox", 10), ["the quick ", "brown fox"]);
        assert_eq!(rows("hello world", 5), ["hello ", "world"]);
        assert_eq!(rows("well-known", 6), ["well-", "known"]);
        assert_eq!(rows("ab漢字", 5), ["ab漢", "字"]);
    }

    #[test]
    fn a_word_wider_than_the_row_breaks_between_graphemes() {
        assert_eq!(rows("a bcdefgh", 4), ["a ", "bcde", "fgh"]);
        assert_eq!(
            rows("e\u{301}e\u{301}e\u{301}", 2),
            ["e\u{301}e\u{301}", "e\u{301}"]
        );
        // A wide grapheme that follows a break still fits its row.
        assert_eq!(rows("a 漢字字", 3), ["a ", "漢", "字", "字"]);
    }

    #[test]
    fn blanks_after_a_word_hang_on_its_row() {
        let layout = layout_line("ab   cd", soft(3));
        assert_eq!(rows("ab   cd", 3), ["ab   ", "cd"]);
        assert_eq!(layout.segments[0].width(), 5);
        assert_eq!(rows("a\tb", 3), ["a\t", "b"]);
    }

    #[test]
    fn unicode_rules_keep_punctuation_and_joined_text_together() {
        // No break comes before closing punctuation or after a no-break space.
        assert_eq!(rows("abc d!", 5), ["abc ", "d!"]);
        assert_eq!(rows("a b\u{a0}c", 4), ["a ", "b\u{a0}c"]);
        // A joined emoji is one grapheme, so no break falls inside it.
        assert_eq!(rows("ab👩\u{200d}💻", 3), ["ab", "👩\u{200d}💻"]);
        // A script that needs dictionary segmentation breaks between
        // graphemes.
        let thai = "สวัสดีครับ";
        let thai_rows = rows(thai, 3);
        assert!(thai_rows.len() > 1);
        assert_eq!(thai_rows.concat(), thai);
    }

    #[test]
    fn edge_widths_and_blank_lines() {
        assert_eq!(rows("", 5), [""]);
        assert_eq!(rows("ab", 0), ["a", "b"]);
        // A wide grapheme wider than the row takes a row of its own.
        assert_eq!(rows("漢字", 1), ["漢", "字"]);
        assert_eq!(rows("abc def", 3), ["abc ", "def"]);
        assert_eq!(rows("abc   def", 3), ["abc   ", "def"]);
        // Blanks before any text fill rows like other graphemes.
        assert_eq!(rows("     ", 3), ["   ", "  "]);
        assert_eq!(rows("\tab", 2), ["\t", "ab"]);
    }

    #[test]
    fn indentation_fills_its_row() {
        assert_eq!(rows("    aaaaaa", 6), ["    aa", "aaaa"]);
        assert_eq!(rows("        ab", 6), ["      ", "  ab"]);
    }

    #[test]
    fn hanging_blanks_at_the_end_add_a_caret_row() {
        assert_eq!(rows("hello", 5), ["hello"]);
        assert_eq!(rows("hello ", 5), ["hello ", ""]);
        assert_eq!(rows("hello ", 6), ["hello "]);
        assert_eq!(rows("ab\t", 3), ["ab\t", ""]);
        // Only hanging blanks add the row: an overwide grapheme does not.
        assert_eq!(rows("漢", 1), ["漢"]);
        let display = layout_line("hello ", Wrap::new(WrapMode::Soft, 5, 4, false));
        assert_eq!(display.segments.len(), 1);
        let unwrapped = layout_line("hello ", Wrap::new(WrapMode::None, 5, 4, true));
        assert_eq!(unwrapped.segments.len(), 1);
    }

    /// Return a line of graphemes that exercise the wrap rules.
    fn line_strategy() -> impl Strategy<Value = String> {
        let pieces = prop::sample::select(vec![
            "a",
            "b",
            "word",
            " ",
            "  ",
            "\t",
            "-",
            "!",
            "漢",
            "e\u{301}",
            "\u{a0}",
            "👩\u{200d}💻",
        ]);
        prop::collection::vec(pieces, 0..24).prop_map(|pieces| pieces.concat())
    }

    proptest! {
        #[test]
        fn segments_cover_the_line_in_order(
            text in line_strategy(),
            width in 0usize..8,
            caret in any::<bool>(),
        ) {
            let wrap = Wrap::new(WrapMode::Soft, width, 4, caret);
            let layout = layout_line(&text, wrap);
            let segments = &layout.segments;
            prop_assert_eq!(segments[0].start_char, 0);
            prop_assert_eq!(segments[0].start_col, 0);
            for pair in segments.windows(2) {
                prop_assert_eq!(pair[0].end_char, pair[1].start_char);
                prop_assert_eq!(pair[0].end_col, pair[1].start_col);
            }
            let last = segments.last().expect("a segment");
            prop_assert_eq!(last.end_char, text.chars().count());
            prop_assert_eq!(last.end_col, layout.display_width);
            // Only the last segment can be empty: the one of an empty line,
            // or the caret row.
            for (index, segment) in segments.iter().enumerate() {
                if segment.start_char == segment.end_char {
                    prop_assert_eq!(index + 1, segments.len());
                    prop_assert!(text.is_empty() || (caret && index > 0));
                }
            }
        }

        #[test]
        fn rows_fit_except_for_hanging_blanks_and_single_graphemes(
            text in line_strategy(),
            width in 1usize..8,
        ) {
            let chars: Vec<char> = text.chars().collect();
            for segment in layout_line(&text, soft(width)).segments {
                let row: String = chars[segment.start_char..segment.end_char].iter().collect();
                let trimmed = row.trim_end_matches([' ', '\t']);
                let mut col = segment.start_col;
                for grapheme in trimmed.graphemes(true) {
                    col += text::cell_width(grapheme, col, 4);
                }
                prop_assert!(
                    col - segment.start_col <= width || trimmed.graphemes(true).count() == 1,
                    "row {row:?} is too wide for {width}",
                );
            }
        }

        #[test]
        fn cached_and_direct_carets_agree_and_stay_in_the_row(
            text in line_strategy(),
            width in 1usize..8,
        ) {
            let mut buffer = TextBuffer::new(text.as_str());
            let mut cache = LayoutCache::new();
            cache.sync(&mut buffer, soft(width));
            let mut column = 0;
            for grapheme in iter::once("").chain(text.graphemes(true)) {
                column += grapheme.chars().count();
                let position = TextPosition::new(0, column);
                let cached = cache.point_for_position(&buffer, position);
                prop_assert_eq!(cached, point_for_position(&buffer, position, soft(width)));
                prop_assert!(cached.x as usize <= width);
            }
        }
    }

    #[test]
    fn the_caret_in_hanging_blanks_stays_within_the_wrap_width() {
        let mut buffer = TextBuffer::new("hello   world");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(5));
        let point = |column| cache.point_for_position(&buffer, TextPosition::new(0, column));
        assert_eq!(point(5), Point { x: 5, y: 0 });
        assert_eq!(point(7), Point { x: 5, y: 0 });
        assert_eq!(point(8), Point { x: 0, y: 1 });

        let mut end = TextBuffer::new("hello ");
        let mut end_cache = LayoutCache::new();
        end_cache.sync(&mut end, soft(5));
        let at_end = end_cache.point_for_position(&end, TextPosition::new(0, 6));
        assert_eq!(at_end, Point { x: 0, y: 1 });
    }

    #[test]
    fn a_row_column_clamps_before_the_blank_that_ends_the_row() {
        let mut buffer = TextBuffer::new("hello   world");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(5));
        assert_eq!(
            cache.position_in_row(&buffer, 0, 20),
            TextPosition::new(0, 5)
        );
        assert_eq!(
            cache.position_in_row(&buffer, 1, 20),
            TextPosition::new(0, 13)
        );
    }

    #[test]
    fn crlf_and_lf_have_equivalent_wrapping() {
        let mut lf = TextBuffer::new("abcd\nefgh\n");
        let mut crlf = TextBuffer::new("abcd\r\nefgh\r\n");
        let mut lf_cache = LayoutCache::new();
        let mut crlf_cache = LayoutCache::new();
        lf_cache.sync(&mut lf, soft(2));
        crlf_cache.sync(&mut crlf, soft(2));
        assert_eq!(lf_cache.lines, crlf_cache.lines);
        assert_eq!(lf_cache.line_offsets, crlf_cache.line_offsets);
    }

    #[test]
    fn mapping_roundtrip() {
        let mut buffer = TextBuffer::new("a\tb");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(10));
        let pos = TextPosition::new(0, 2);
        let point = cache.point_for_position(&buffer, pos);
        let back = cache.position_in_row(&buffer, point.y as usize, point.x as usize);
        assert_eq!(back, pos);
    }

    #[test]
    fn line_for_display_accounts_for_wrapping() {
        let mut buffer = TextBuffer::new("aa\naaa");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(2));
        assert_eq!(cache.total_lines(), 3);
        assert_eq!(cache.line_for_display(0), 0);
        assert_eq!(cache.line_for_display(1), 1);
        assert_eq!(cache.line_for_display(2), 1);
    }

    #[test]
    fn position_in_row_clamps_to_segment() {
        let mut buffer = TextBuffer::new("hello");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(3));
        // The end of the first row shows at the start of the second, so the
        // column stops before its last grapheme.
        assert_eq!(
            cache.position_in_row(&buffer, 0, 10),
            TextPosition::new(0, 2)
        );
        assert_eq!(
            cache.position_in_row(&buffer, 1, 10),
            TextPosition::new(0, 5)
        );
    }

    #[test]
    fn sync_rebuilds_after_three_edits_between_syncs() {
        let mut buffer = TextBuffer::new("aaaa\nbbbb\ncccc");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, soft(4));

        for line in 0..3 {
            let at = TextPosition::new(line, 0);
            buffer.replace_range(TextRange::new(at, at), "X");
        }
        cache.sync(&mut buffer, soft(4));

        let mut rebuilt = LayoutCache::new();
        rebuilt.sync(&mut buffer, soft(4));
        assert_eq!(cache.total_lines(), rebuilt.total_lines());
        assert_eq!(
            cache.metrics_for(&buffer, soft(4)),
            rebuilt.metrics_for(&buffer, soft(4))
        );
        for i in 0..buffer.line_count() {
            assert_eq!(cache.line(i), rebuilt.line(i), "line {i}");
        }
    }
}
