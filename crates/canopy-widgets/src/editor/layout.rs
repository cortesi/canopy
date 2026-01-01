use canopy::{geom::Point, text};
use unicode_segmentation::UnicodeSegmentation;

use super::{LineChange, TextBuffer, TextPosition, WrapMode, tab_width};

/// A wrapped segment of a logical line.
#[derive(Debug, Clone)]
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
    /// Return the display width of this segment.
    pub fn width(&self) -> usize {
        self.end_col.saturating_sub(self.start_col)
    }
}

/// Layout information for a single logical line.
#[derive(Debug, Clone)]
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
    /// Cached wrap width.
    wrap_width: usize,
    /// Cached wrap mode.
    wrap_mode: WrapMode,
    /// Cached tab stop.
    tab_stop: usize,
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
            wrap_width: 0,
            wrap_mode: WrapMode::None,
            tab_stop: 4,
            revision: 0,
        }
    }

    /// Synchronize layout state with the buffer and wrapping parameters.
    pub fn sync(
        &mut self,
        buffer: &mut TextBuffer,
        wrap_width: usize,
        wrap_mode: WrapMode,
        tab_stop: usize,
    ) {
        let wrap_width = wrap_width.max(1);
        let needs_rebuild = self.wrap_width != wrap_width
            || self.wrap_mode != wrap_mode
            || self.tab_stop != tab_stop;

        if needs_rebuild {
            self.rebuild_all(buffer, wrap_width, wrap_mode, tab_stop);
            return;
        }

        let revision = buffer.revision();
        if revision == self.revision {
            return;
        }

        if let Some(change) = buffer.take_change() {
            self.apply_change(buffer, change, wrap_width, wrap_mode, tab_stop);
        } else {
            self.rebuild_all(buffer, wrap_width, wrap_mode, tab_stop);
        }

        self.revision = revision;
    }

    /// Return the total number of display lines.
    pub fn total_lines(&self) -> usize {
        self.total_lines
    }

    /// Return the maximum display width.
    pub fn max_line_width(&self) -> usize {
        self.max_line_width
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
    pub fn point_for_position(
        &self,
        buffer: &TextBuffer,
        position: TextPosition,
        tab_stop: usize,
    ) -> Point {
        let line = position.line.min(self.lines.len().saturating_sub(1));
        let layout = self.lines.get(line);
        if layout.is_none() {
            return Point { x: 0, y: 0 };
        }
        let layout = layout.expect("layout present");
        let display_col = buffer.column_for_position(position, tab_stop);
        let seg_idx = layout.segment_for_column(display_col);
        let seg = layout.segment(seg_idx).expect("segment present");
        let x = display_col.saturating_sub(seg.start_col);
        let y = self.line_offset(line).saturating_add(seg_idx);
        Point {
            x: x as u32,
            y: y as u32,
        }
    }

    /// Map display coordinates to the closest text position.
    pub fn position_for_point(
        &self,
        buffer: &TextBuffer,
        point: Point,
        tab_stop: usize,
    ) -> TextPosition {
        if self.lines.is_empty() {
            return TextPosition::new(0, 0);
        }
        let y = point.y as usize;
        let line_idx = self.line_for_display(y);
        let line_layout = self.lines.get(line_idx);
        if line_layout.is_none() {
            return TextPosition::new(0, 0);
        }
        let line_layout = line_layout.expect("line layout present");
        let line_start = self.line_offset(line_idx);
        let seg_idx = y
            .saturating_sub(line_start)
            .min(line_layout.segments.len().saturating_sub(1));
        let seg = line_layout.segment(seg_idx).expect("segment present");
        let mut col = point.x as usize;
        let seg_width = seg.width();
        if col > seg_width {
            col = seg_width;
        }
        let display_col = seg.start_col.saturating_add(col);
        buffer.position_for_column(line_idx, display_col, tab_stop)
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
    fn rebuild_all(
        &mut self,
        buffer: &TextBuffer,
        wrap_width: usize,
        wrap_mode: WrapMode,
        tab_stop: usize,
    ) {
        self.lines.clear();
        let line_count = buffer.line_count().max(1);
        for line in 0..line_count {
            let text = buffer.line_text(line);
            let layout = layout_line(&text, wrap_mode, wrap_width, tab_stop);
            self.lines.push(layout);
        }
        self.rebuild_offsets();
        self.wrap_width = wrap_width;
        self.wrap_mode = wrap_mode;
        self.tab_stop = tab_stop;
        self.revision = buffer.revision();
    }

    /// Apply an incremental line change to the layout cache.
    fn apply_change(
        &mut self,
        buffer: &TextBuffer,
        change: LineChange,
        wrap_width: usize,
        wrap_mode: WrapMode,
        tab_stop: usize,
    ) {
        let start = change.start_line.min(self.lines.len());
        let end = start
            .saturating_add(change.old_line_count)
            .min(self.lines.len());
        let mut replacement = Vec::new();
        let new_end = start.saturating_add(change.new_line_count);
        for line in start..new_end {
            let text = buffer.line_text(line);
            let layout = layout_line(&text, wrap_mode, wrap_width, tab_stop);
            replacement.push(layout);
        }
        self.lines.splice(start..end, replacement);
        if self.lines.is_empty() {
            self.lines
                .push(layout_line("", wrap_mode, wrap_width, tab_stop));
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

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Build layout segments for a single logical line.
pub fn layout_line(
    text: &str,
    wrap_mode: WrapMode,
    wrap_width: usize,
    tab_stop: usize,
) -> LineLayout {
    let wrap_width = wrap_width.max(1);
    let mut segments = Vec::new();
    let mut col = 0usize;
    let mut char_index = 0usize;
    let mut seg_start_char = 0usize;
    let mut seg_start_col = 0usize;

    if text.is_empty() {
        segments.push(WrapSegment {
            start_char: 0,
            end_char: 0,
            start_col: 0,
            end_col: 0,
        });
        return LineLayout {
            segments,
            display_width: 0,
        };
    }

    for grapheme in text.graphemes(true) {
        let grapheme_chars = grapheme.chars().count();
        let width = if grapheme == "\t" {
            tab_width(col, tab_stop)
        } else {
            text::grapheme_width(grapheme)
        };

        if wrap_mode == WrapMode::Soft {
            let seg_width = col.saturating_sub(seg_start_col);
            if seg_width > 0 && seg_width.saturating_add(width) > wrap_width {
                segments.push(WrapSegment {
                    start_char: seg_start_char,
                    end_char: char_index,
                    start_col: seg_start_col,
                    end_col: col,
                });
                seg_start_char = char_index;
                seg_start_col = col;
            }
        }

        col = col.saturating_add(width);
        char_index = char_index.saturating_add(grapheme_chars);
    }

    segments.push(WrapSegment {
        start_char: seg_start_char,
        end_char: char_index,
        start_col: seg_start_col,
        end_col: col,
    });

    LineLayout {
        segments,
        display_width: col,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_layout_splits_lines() {
        let line = "hello";
        let layout = layout_line(line, WrapMode::Soft, 2, 4);
        assert_eq!(layout.segments.len(), 3);
        assert_eq!(layout.display_width, 5);
    }

    #[test]
    fn mapping_roundtrip() {
        let mut buffer = TextBuffer::new("a\tb");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, 10, WrapMode::Soft, 4);
        let pos = TextPosition::new(0, 2);
        let point = cache.point_for_position(&buffer, pos, 4);
        let back = cache.position_for_point(&buffer, point, 4);
        assert_eq!(back, pos);
    }

    #[test]
    fn line_for_display_accounts_for_wrapping() {
        let mut buffer = TextBuffer::new("aa\naaa");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, 2, WrapMode::Soft, 4);
        assert_eq!(cache.total_lines(), 3);
        assert_eq!(cache.line_for_display(0), 0);
        assert_eq!(cache.line_for_display(1), 1);
        assert_eq!(cache.line_for_display(2), 1);
    }

    #[test]
    fn position_for_point_clamps_to_segment() {
        let mut buffer = TextBuffer::new("hello");
        let mut cache = LayoutCache::new();
        cache.sync(&mut buffer, 3, WrapMode::Soft, 4);
        let point = Point { x: 10, y: 0 };
        let pos = cache.position_for_point(&buffer, point, 4);
        assert_eq!(pos, TextPosition::new(0, 3));
    }
}
