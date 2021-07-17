use super::{Point, Rect};
use crate::{Error, Result};

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
    pub fn new(rect: Rect, border: u16) -> Result<Self> {
        if rect.w < (border * 2) || rect.h < (border * 2) {
            return Err(Error::Geometry("rectangle too small".into()));
        }
        Ok(Frame {
            top: Rect {
                tl: Point {
                    x: rect.tl.x + border,
                    y: rect.tl.y,
                },
                w: rect.w - 2 * border,
                h: border,
            },
            bottom: Rect {
                tl: Point {
                    x: rect.tl.x + border,
                    y: rect.tl.y + rect.h - border,
                },
                w: rect.w - 2 * border,
                h: border,
            },
            left: Rect {
                tl: Point {
                    x: rect.tl.x,
                    y: rect.tl.y + border,
                },
                w: border,
                h: rect.h - 2 * border,
            },
            right: Rect {
                tl: Point {
                    x: rect.tl.x + rect.w - border,
                    y: rect.tl.y + border,
                },
                w: border,
                h: rect.h - 2 * border,
            },
            topleft: Rect {
                tl: Point {
                    x: rect.tl.x,
                    y: rect.tl.y,
                },
                w: border,
                h: border,
            },
            topright: Rect {
                tl: Point {
                    x: rect.tl.x + rect.w - border,
                    y: rect.tl.y,
                },
                w: border,
                h: border,
            },
            bottomleft: Rect {
                tl: Point {
                    x: rect.tl.x,
                    y: rect.tl.y + rect.h - border,
                },
                w: border,
                h: border,
            },
            bottomright: Rect {
                tl: Point {
                    x: rect.tl.x + rect.w - border,
                    y: rect.tl.y + rect.h - border,
                },
                w: border,
                h: border,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tframe() -> Result<()> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 10,
            h: 10,
        };
        assert_eq!(
            Frame::new(r, 1)?,
            Frame {
                top: Rect {
                    tl: Point { x: 11, y: 10 },
                    w: 8,
                    h: 1
                },
                bottom: Rect {
                    tl: Point { x: 11, y: 19 },
                    w: 8,
                    h: 1
                },
                left: Rect {
                    tl: Point { x: 10, y: 11 },
                    w: 1,
                    h: 8
                },
                right: Rect {
                    tl: Point { x: 19, y: 11 },
                    w: 1,
                    h: 8
                },
                topleft: Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 1,
                    h: 1
                },
                topright: Rect {
                    tl: Point { x: 19, y: 10 },
                    w: 1,
                    h: 1
                },
                bottomleft: Rect {
                    tl: Point { x: 10, y: 19 },
                    w: 1,
                    h: 1
                },
                bottomright: Rect {
                    tl: Point { x: 19, y: 19 },
                    w: 1,
                    h: 1
                },
            }
        );
        Ok(())
    }
}
