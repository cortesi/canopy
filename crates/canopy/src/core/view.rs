use crate::{
    error::Result,
    geom::{Point, PointI32, Rect, RectI32, Size},
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
    pub canvas: Size,
}

impl View {
    /// Convert a screen point to viewport-local coordinates, before scroll.
    /// Coordinates outside the signed range saturate at its limits.
    pub fn screen_to_viewport(&self, point: PointI32) -> PointI32 {
        signed_point(
            i64::from(point.x) - i64::from(self.content.tl.x),
            i64::from(point.y) - i64::from(self.content.tl.y),
        )
    }

    /// Convert a viewport-local point to scrolled content coordinates.
    /// Coordinates outside the signed range saturate at its limits.
    pub fn viewport_to_content(&self, point: PointI32) -> PointI32 {
        signed_point(
            i64::from(point.x) + i64::from(self.tl.x),
            i64::from(point.y) + i64::from(self.tl.y),
        )
    }

    /// Convert a viewport-local point to outer-local coordinates, including padding.
    /// Coordinates outside the signed range saturate at its limits.
    pub fn viewport_to_outer(&self, point: PointI32) -> PointI32 {
        signed_point(
            i64::from(point.x) + i64::from(self.content.tl.x) - i64::from(self.outer.tl.x),
            i64::from(point.y) + i64::from(self.content.tl.y) - i64::from(self.outer.tl.y),
        )
    }

    /// Convert a scrolled content point to screen coordinates.
    /// Coordinates outside the signed range saturate at its limits.
    pub fn content_to_screen(&self, point: PointI32) -> PointI32 {
        signed_point(
            i64::from(point.x) + i64::from(self.content.tl.x) - i64::from(self.tl.x),
            i64::from(point.y) + i64::from(self.content.tl.y) - i64::from(self.tl.y),
        )
    }

    /// Convert an outer-local point to scrolled content coordinates.
    /// Coordinates outside the signed range saturate at its limits.
    pub fn outer_to_content(&self, point: PointI32) -> PointI32 {
        signed_point(
            i64::from(point.x) + i64::from(self.outer.tl.x) - i64::from(self.content.tl.x)
                + i64::from(self.tl.x),
            i64::from(point.y) + i64::from(self.outer.tl.y) - i64::from(self.content.tl.y)
                + i64::from(self.tl.y),
        )
    }

    /// Size of the outer rect.
    pub fn outer_size(&self) -> Size {
        Size::new(self.outer.w, self.outer.h)
    }

    /// Size of the content rect.
    pub fn content_size(&self) -> Size {
        Size::new(self.content.w, self.content.h)
    }

    /// True if the view is zero-sized.
    pub fn is_zero(&self) -> bool {
        self.outer.w == 0 || self.outer.h == 0
    }

    /// Offset from the outer origin to the content origin, in local
    /// coordinates.
    pub fn content_origin(&self) -> Point {
        let dx = (i64::from(self.content.tl.x) - i64::from(self.outer.tl.x)).max(0) as u32;
        let dy = (i64::from(self.content.tl.y) - i64::from(self.outer.tl.y)).max(0) as u32;
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

    /// Build a view from signed outer and content rects, a scroll offset, and a
    /// canvas size.
    pub fn new(outer: RectI32, content: RectI32, tl: Point, canvas: Size) -> Self {
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
                margin.vslice(pre)?,
                margin.vslice(active)?,
                margin.vslice(post)?,
            )))
        }
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a
    /// horizontal scroll bar for this view in the specified margin rect.
    pub fn hactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        let view = self.view_rect();
        if view.w == self.canvas.w {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .hextent()
                .split_active(view.hextent(), self.canvas.rect().hextent())?;
            Ok(Some((
                margin.hslice(pre)?,
                margin.hslice(active)?,
                margin.hslice(post)?,
            )))
        }
    }
}

/// Clamp only after the complete translation, so intermediate offsets retain their sign.
fn signed_point(x: i64, y: i64) -> PointI32 {
    PointI32 {
        x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn view_for_sizes(content: Size, canvas: Size, tl: Point) -> View {
        View::new(
            RectI32::new(0, 0, content.w, content.h),
            RectI32::new(0, 0, content.w, content.h),
            tl,
            canvas,
        )
    }

    fn vertical_part_fits_margin(part: Rect, margin: Rect) -> bool {
        if part.h == 0 {
            part.tl.x == margin.tl.x
                && part.w == margin.w
                && part.tl.y >= margin.tl.y
                && part.tl.y <= margin.tl.y.saturating_add(margin.h)
        } else {
            margin.contains_rect(part)
        }
    }

    fn horizontal_part_fits_margin(part: Rect, margin: Rect) -> bool {
        if part.w == 0 {
            part.tl.y == margin.tl.y
                && part.h == margin.h
                && part.tl.x >= margin.tl.x
                && part.tl.x <= margin.tl.x.saturating_add(margin.w)
        } else {
            margin.contains_rect(part)
        }
    }

    proptest! {
        #[test]
        fn vertical_scrollbar_parts_cover_margin(
            content_h in 1u32..50,
            extra_h in 1u32..50,
            raw_scroll_y in 0u32..100,
            margin_h in 1u32..50,
        ) {
            let content = Size::new(4, content_h);
            let canvas = Size::new(4, content_h.saturating_add(extra_h));
            let scroll_y = raw_scroll_y.min(canvas.h.saturating_sub(content.h));
            let view = view_for_sizes(content, canvas, Point { x: 0, y: scroll_y });
            let margin = Rect::new(7, 3, 1, margin_h);

            let Some((pre, active, post)) = view.vactive(margin).unwrap() else {
                panic!("larger canvas should produce a vertical scrollbar");
            };

            prop_assert!(vertical_part_fits_margin(pre, margin));
            prop_assert!(vertical_part_fits_margin(active, margin));
            prop_assert!(vertical_part_fits_margin(post, margin));
            prop_assert_eq!(pre.h.saturating_add(active.h).saturating_add(post.h), margin.h);
            prop_assert_eq!(pre.tl.y, margin.tl.y);
            prop_assert_eq!(active.tl.y, pre.tl.y.saturating_add(pre.h));
            prop_assert_eq!(post.tl.y, active.tl.y.saturating_add(active.h));
        }

        #[test]
        fn horizontal_scrollbar_parts_cover_margin(
            content_w in 1u32..50,
            extra_w in 1u32..50,
            raw_scroll_x in 0u32..100,
            margin_w in 1u32..50,
        ) {
            let content = Size::new(content_w, 4);
            let canvas = Size::new(content_w.saturating_add(extra_w), 4);
            let scroll_x = raw_scroll_x.min(canvas.w.saturating_sub(content.w));
            let view = view_for_sizes(content, canvas, Point { x: scroll_x, y: 0 });
            let margin = Rect::new(3, 7, margin_w, 1);

            let Some((pre, active, post)) = view.hactive(margin).unwrap() else {
                panic!("larger canvas should produce a horizontal scrollbar");
            };

            prop_assert!(horizontal_part_fits_margin(pre, margin));
            prop_assert!(horizontal_part_fits_margin(active, margin));
            prop_assert!(horizontal_part_fits_margin(post, margin));
            prop_assert_eq!(pre.w.saturating_add(active.w).saturating_add(post.w), margin.w);
            prop_assert_eq!(pre.tl.x, margin.tl.x);
            prop_assert_eq!(active.tl.x, pre.tl.x.saturating_add(pre.w));
            prop_assert_eq!(post.tl.x, active.tl.x.saturating_add(active.w));
        }
    }

    #[test]
    fn scrollbars_are_absent_when_canvas_matches_view() {
        let view = view_for_sizes(Size::new(10, 5), Size::new(10, 5), Point::zero());
        assert!(view.vactive(Rect::new(0, 0, 1, 5)).unwrap().is_none());
        assert!(view.hactive(Rect::new(0, 0, 10, 1)).unwrap().is_none());
    }
    #[test]
    fn content_origins_cover_the_signed_coordinate_range() {
        for (outer, content, expected) in [
            (0, 0, 0),
            (8, 3, 0),
            (-8, -3, 5),
            (i32::MIN, i32::MAX, u32::MAX),
        ] {
            let view = View::new(
                RectI32::new(outer, outer, 10, 10),
                RectI32::new(content, content, 4, 3),
                Point::zero(),
                Size::new(4, 3),
            );
            assert_eq!(
                view.content_origin(),
                Point {
                    x: expected,
                    y: expected
                }
            );
            assert_eq!(view.view_rect_local(), Rect::new(expected, expected, 4, 3));
        }
    }
}
