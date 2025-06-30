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
}

impl Frame {
    /// Construct a new frame. If the rect is too small to fit the specified
    /// frame, we return a zero Frame.
    pub fn new(rect: Rect, border: u16) -> Self {
        if rect.w < (border * 2) || rect.h < (border * 2) {
            Frame::zero()
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
            }
        }
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
            }
        );
        Ok(())
    }
}
