use super::Rect;

/// A frame's border regions extracted from a rectangle.
///
/// This struct represents the decomposition of a rectangle into its border
/// regions: top, bottom, left, right, and corner rectangles. It's useful for
/// drawing box borders or frame decorations.
#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub struct FrameRects {
    /// The top of the frame, not including corners
    pub top: Rect,
    /// The bottom of the frame, not including corners
    pub bottom: Rect,
    /// The left of the frame, not including corners
    pub left: Rect,
    /// The right of the frame, not including corners
    pub right: Rect,
    /// The top left corner
    pub topleft: Rect,
    /// The top right corner
    pub topright: Rect,
    /// The bottom left corner
    pub bottomleft: Rect,
    /// The bottom right corner
    pub bottomright: Rect,
    /// The space inside the frame
    pub inner: Rect,
}

impl FrameRects {
    /// Construct a new frame. If the rect is too small to fit the specified
    /// frame, we return an all-zero FrameRects.
    pub fn new(rect: Rect, border: u32) -> Self {
        let Some(border_width) = border.checked_mul(2) else {
            return Self::default();
        };
        if rect.w <= border_width || rect.h <= border_width {
            return Self::default();
        }
        Self {
            top: Rect::new(
                rect.tl.x.saturating_add(border),
                rect.tl.y,
                rect.w - border_width,
                border,
            ),
            bottom: Rect::new(
                rect.tl.x.saturating_add(border),
                rect.tl.y.saturating_add(rect.h).saturating_sub(border),
                rect.w - border_width,
                border,
            ),
            left: Rect::new(
                rect.tl.x,
                rect.tl.y.saturating_add(border),
                border,
                rect.h - border_width,
            ),
            right: Rect::new(
                rect.tl.x.saturating_add(rect.w).saturating_sub(border),
                rect.tl.y.saturating_add(border),
                border,
                rect.h - border_width,
            ),
            topleft: Rect::new(rect.tl.x, rect.tl.y, border, border),
            topright: Rect::new(
                rect.tl.x.saturating_add(rect.w).saturating_sub(border),
                rect.tl.y,
                border,
                border,
            ),
            bottomleft: Rect::new(
                rect.tl.x,
                rect.tl.y.saturating_add(rect.h).saturating_sub(border),
                border,
                border,
            ),
            bottomright: Rect::new(
                rect.tl.x.saturating_add(rect.w).saturating_sub(border),
                rect.tl.y.saturating_add(rect.h).saturating_sub(border),
                border,
                border,
            ),
            inner: Rect::new(
                rect.tl.x.saturating_add(border),
                rect.tl.y.saturating_add(border),
                rect.w - border_width,
                rect.h - border_width,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn tframe() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        assert_eq!(
            FrameRects::new(r, 1),
            FrameRects {
                top: Rect::new(11, 10, 8, 1),
                bottom: Rect::new(11, 19, 8, 1),
                left: Rect::new(10, 11, 1, 8),
                right: Rect::new(19, 11, 1, 8),
                topleft: Rect::new(10, 10, 1, 1),
                topright: Rect::new(19, 10, 1, 1),
                bottomleft: Rect::new(10, 19, 1, 1),
                bottomright: Rect::new(19, 19, 1, 1),
                inner: Rect::new(11, 11, 8, 8),
            }
        );
        Ok(())
    }

    #[test]
    fn frames_too_small_for_their_border_are_all_zero() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        assert_eq!(FrameRects::new(r, 2).inner, Rect::new(12, 12, 6, 6));

        // An exact fit leaves no inner space.
        assert_eq!(FrameRects::new(r, 5), FrameRects::default());
        // A border wider than the rect leaves no frame at all.
        assert_eq!(FrameRects::new(r, 6), FrameRects::default());
        // A border whose doubled width overflows is also rejected.
        assert_eq!(FrameRects::new(r, u32::MAX), FrameRects::default());

        Ok(())
    }
}
