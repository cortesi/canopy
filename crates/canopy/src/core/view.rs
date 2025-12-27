use crate::{
    error::Result,
    geom::{Expanse, Point, Rect, RectI32},
};

/// Render-time view information for a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// Outer rect in screen coordinates (signed for scroll translations).
    pub outer: RectI32,
    /// Content rect in screen coordinates (outer inset by padding).
    pub content: RectI32,
    /// Viewport offset in content coordinates (scroll position).
    pub tl: Point,
    /// Canvas size in content coordinates.
    pub canvas: Expanse,
}

impl View {
    /// Size of the outer rect.
    pub fn outer_size(&self) -> Expanse {
        Expanse::new(self.outer.w, self.outer.h)
    }

    /// Size of the content rect.
    pub fn content_size(&self) -> Expanse {
        Expanse::new(self.content.w, self.content.h)
    }

    /// True if the view is zero-sized.
    pub fn is_zero(&self) -> bool {
        self.outer.w == 0 || self.outer.h == 0
    }

    /// Offset from the outer origin to the content origin, in local coordinates.
    pub fn content_origin(&self) -> Point {
        let dx = (self.content.tl.x - self.outer.tl.x).max(0) as u32;
        let dy = (self.content.tl.y - self.outer.tl.y).max(0) as u32;
        Point { x: dx, y: dy }
    }

    /// Visible view rectangle in content coordinates.
    pub fn view_rect(&self) -> Rect {
        Rect::new(self.tl.x, self.tl.y, self.content.w, self.content.h)
    }

    /// Visible view rectangle in local outer coordinates.
    pub fn view_rect_local(&self) -> Rect {
        let origin = self.content_origin();
        Rect::new(origin.x, origin.y, self.content.w, self.content.h)
    }

    /// Local outer rectangle with origin at (0,0).
    pub fn outer_rect_local(&self) -> Rect {
        Rect::new(0, 0, self.outer.w, self.outer.h)
    }

    /// Build a view from signed outer/content rects and content/canvas sizes.
    pub fn new(outer: RectI32, content: RectI32, tl: Point, canvas: Expanse) -> Self {
        Self {
            outer,
            content,
            tl,
            canvas,
        }
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a vertical
    /// scroll bar for this view in the specified margin rect.
    pub fn vactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        let view = self.view_rect();
        if view.h == self.canvas.h {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .vextent()
                .split_active(view.vextent(), self.canvas.rect().vextent())?;
            Ok(Some((
                margin.vslice(&pre)?,
                margin.vslice(&active)?,
                margin.vslice(&post)?,
            )))
        }
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a horizontal
    /// scroll bar for this view in the specified margin rect.
    pub fn hactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        let view = self.view_rect();
        if view.w == self.canvas.w {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .hextent()
                .split_active(view.hextent(), self.canvas.rect().hextent())?;
            Ok(Some((
                margin.hslice(&pre)?,
                margin.hslice(&active)?,
                margin.hslice(&post)?,
            )))
        }
    }
}
