use canopy::geom::{Point, Rect};

use super::{TextBuffer, WrapMode, layout::LayoutCache};

/// Cached editor view state derived from layout and cursor position.
#[derive(Debug, Clone)]
pub struct EditorView {
    /// Layout cache for wrapping and mapping.
    pub(crate) layout: LayoutCache,
    /// Cached cursor position in content coordinates.
    pub(crate) cursor_point: Option<Point>,
    /// Cached cursor position in view coordinates.
    pub(crate) cursor_view_point: Option<Point>,
}

impl EditorView {
    /// Construct empty view state.
    pub(crate) fn new() -> Self {
        Self {
            layout: LayoutCache::new(),
            cursor_point: None,
            cursor_view_point: None,
        }
    }

    /// Synchronize layout and cached cursor position.
    pub(crate) fn sync(
        &mut self,
        buffer: &mut TextBuffer,
        view_rect: Rect,
        gutter_width: u32,
        wrap_width: usize,
        wrap: WrapMode,
        tab_stop: usize,
    ) {
        self.layout.sync(buffer, wrap_width, wrap, tab_stop);
        let cursor = buffer.cursor();
        let point = self.layout.point_for_position(buffer, cursor, tab_stop);
        let cursor_point = Point {
            x: point.x.saturating_add(gutter_width),
            y: point.y,
        };
        self.cursor_point = Some(cursor_point);
        self.cursor_view_point = if view_rect.contains_point(cursor_point) {
            Some(Point {
                x: cursor_point.x - view_rect.tl.x,
                y: cursor_point.y - view_rect.tl.y,
            })
        } else {
            None
        };
    }
}
