use super::Rect;

/// A frame extracted from a rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Frame {
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
    /// The original outer rect
    outer_rect: Rect,
    /// The border width
    border: u32,
}

impl Frame {
    /// Construct a new frame. If the rect is too small to fit the specified
    /// frame, we return a zero Frame.
    pub fn new(rect: Rect, border: u32) -> Self {
        if rect.w <= (border * 2) || rect.h <= (border * 2) {
            let mut f = Frame::zero();
            f.outer_rect = rect;
            f.border = border;
            f
        } else {
            Frame {
                top: Rect::new(rect.tl.x + border, rect.tl.y, rect.w - 2 * border, border),
                bottom: Rect::new(
                    rect.tl.x + border,
                    rect.tl.y + rect.h - border,
                    rect.w - 2 * border,
                    border,
                ),
                left: Rect::new(rect.tl.x, rect.tl.y + border, border, rect.h - 2 * border),
                right: Rect::new(
                    rect.tl.x + rect.w - border,
                    rect.tl.y + border,
                    border,
                    rect.h - 2 * border,
                ),
                topleft: Rect::new(rect.tl.x, rect.tl.y, border, border),
                topright: Rect::new(rect.tl.x + rect.w - border, rect.tl.y, border, border),
                bottomleft: Rect::new(rect.tl.x, rect.tl.y + rect.h - border, border, border),
                bottomright: Rect::new(
                    rect.tl.x + rect.w - border,
                    rect.tl.y + rect.h - border,
                    border,
                    border,
                ),
                outer_rect: rect,
                border,
            }
        }
    }

    /// Get the inner rect of the frame (the space inside the frame)
    pub fn inner(&self) -> Rect {
        if self.outer_rect.w <= (self.border * 2) || self.outer_rect.h <= (self.border * 2) {
            Rect::zero()
        } else {
            Rect::new(
                self.outer_rect.tl.x + self.border,
                self.outer_rect.tl.y + self.border,
                self.outer_rect.w - 2 * self.border,
                self.outer_rect.h - 2 * self.border,
            )
        }
    }

    /// Get the outer rect of the frame (the original rect passed to Frame::new())
    pub fn outer(&self) -> Rect {
        self.outer_rect
    }
    pub fn zero() -> Self {
        Frame {
            top: Rect::zero(),
            bottom: Rect::zero(),
            left: Rect::zero(),
            right: Rect::zero(),
            topleft: Rect::zero(),
            topright: Rect::zero(),
            bottomleft: Rect::zero(),
            bottomright: Rect::zero(),
            outer_rect: Rect::zero(),
            border: 0,
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
            Frame::new(r, 1),
            Frame {
                top: Rect::new(11, 10, 8, 1),
                bottom: Rect::new(11, 19, 8, 1),
                left: Rect::new(10, 11, 1, 8),
                right: Rect::new(19, 11, 1, 8),
                topleft: Rect::new(10, 10, 1, 1),
                topright: Rect::new(19, 10, 1, 1),
                bottomleft: Rect::new(10, 19, 1, 1),
                bottomright: Rect::new(19, 19, 1, 1),
                outer_rect: r,
                border: 1,
            }
        );
        Ok(())
    }

    #[test]
    fn test_inner_outer() -> Result<()> {
        let r = Rect::new(10, 10, 10, 10);
        let frame = Frame::new(r, 1);

        // Test outer rect
        assert_eq!(frame.outer(), r);

        // Test inner rect
        assert_eq!(frame.inner(), Rect::new(11, 11, 8, 8));

        // Test with larger border
        let frame2 = Frame::new(r, 2);
        assert_eq!(frame2.outer(), r);
        assert_eq!(frame2.inner(), Rect::new(12, 12, 6, 6));

        // Test with border too large (zero frame)
        let frame3 = Frame::new(r, 5);
        assert_eq!(frame3.outer(), r); // outer rect is preserved
        assert_eq!(frame3.inner(), Rect::zero());

        // Test with exact fit (border * 2 == dimensions)
        let frame4 = Frame::new(r, 5);
        assert_eq!(frame4.outer(), r); // outer rect is preserved
        assert_eq!(frame4.inner(), Rect::zero());

        Ok(())
    }
}
