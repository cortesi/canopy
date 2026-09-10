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
    pub scroll: Point,
    /// Canvas size in content coordinates.
    pub canvas: Size,
}

impl View {
    /// Convert a screen point to viewport-local coordinates, before scroll.
    /// Returns a geometry error if the result is outside the signed range.
    pub fn screen_to_viewport(&self, point: PointI32) -> Result<PointI32> {
        Ok(PointI32::try_from_i64(
            i64::from(point.x) - i64::from(self.content.tl.x),
            i64::from(point.y) - i64::from(self.content.tl.y),
        )?)
    }

    /// Convert a viewport-local point to scrolled content coordinates.
    /// Returns a geometry error if the result is outside the signed range.
    pub fn viewport_to_content(&self, point: PointI32) -> Result<PointI32> {
        Ok(PointI32::try_from_i64(
            i64::from(point.x) + i64::from(self.scroll.x),
            i64::from(point.y) + i64::from(self.scroll.y),
        )?)
    }

    /// Convert a viewport-local point to outer-local coordinates, including
    /// padding. Returns a geometry error if the result is outside the signed
    /// range.
    pub fn viewport_to_outer(&self, point: PointI32) -> Result<PointI32> {
        Ok(PointI32::try_from_i64(
            i64::from(point.x) + i64::from(self.content.tl.x) - i64::from(self.outer.tl.x),
            i64::from(point.y) + i64::from(self.content.tl.y) - i64::from(self.outer.tl.y),
        )?)
    }

    /// Size of the outer rect.
    pub fn outer_size(&self) -> Size {
        self.outer.size()
    }

    /// Size of the content rect.
    pub fn content_size(&self) -> Size {
        self.content.size()
    }

    /// True if the view has no visible cells.
    pub fn is_empty(&self) -> bool {
        self.outer.is_empty()
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
        Rect::new(self.scroll.x, self.scroll.y, self.content.w, self.content.h)
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
    pub fn new(outer: RectI32, content: RectI32, scroll: Point, canvas: Size) -> Self {
        Self {
            outer,
            content,
            scroll,
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{error::Error, geom::Error as GeomError};

    #[test]
    fn signed_coordinate_conversions_preserve_padding_scroll_and_offscreen_points() {
        let view = View::new(
            RectI32::new(10, 4, 12, 8),
            RectI32::new(11, 5, 10, 6),
            Point { x: 0, y: 3 },
            Size::new(30, 20),
        );
        let screen = PointI32 { x: 13, y: 6 };
        let viewport = view.screen_to_viewport(screen).unwrap();
        assert_eq!(viewport, PointI32 { x: 2, y: 1 });
        assert_eq!(
            view.viewport_to_outer(viewport).unwrap(),
            PointI32 { x: 3, y: 2 }
        );
        let content = view.viewport_to_content(viewport).unwrap();
        assert_eq!(content, PointI32 { x: 2, y: 4 });
        assert_eq!(
            view.screen_to_viewport(PointI32 { x: 9, y: 3 }).unwrap(),
            PointI32 { x: -2, y: -2 },
        );
    }

    #[test]
    fn signed_conversions_check_complete_translation() {
        let view = View::new(
            RectI32::new(i32::MIN, i32::MIN, 10, 10),
            RectI32::new(i32::MIN + 1, i32::MIN + 1, 8, 8),
            Point {
                x: u32::MAX,
                y: u32::MAX,
            },
            Size::new(u32::MAX, u32::MAX),
        );
        assert!(matches!(
            view.screen_to_viewport(PointI32 {
                x: i32::MAX,
                y: i32::MAX
            }),
            Err(Error::Geometry(GeomError::CoordinateOutOfRange { .. })),
        ));
        assert!(view.viewport_to_content(PointI32::default()).is_err());
        assert!(
            view.viewport_to_outer(PointI32 { x: i32::MAX, y: 0 })
                .is_err()
        );
        assert_eq!(
            view.viewport_to_content(PointI32 {
                x: i32::MIN,
                y: i32::MIN
            })
            .unwrap(),
            PointI32 {
                x: i32::MAX,
                y: i32::MAX
            },
        );
        assert_eq!(
            view.viewport_to_outer(PointI32::default()).unwrap(),
            PointI32 { x: 1, y: 1 },
        );
    }

    fn view_for_sizes(content: Size, canvas: Size, scroll: Point) -> View {
        View::new(
            RectI32::new(0, 0, content.w, content.h),
            RectI32::new(0, 0, content.w, content.h),
            scroll,
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
        let view = view_for_sizes(Size::new(10, 5), Size::new(10, 5), Point::ZERO);
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
                Point::ZERO,
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
