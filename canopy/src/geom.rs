use crate::error::CanopyError;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

/// An exctent is a directionless 2 dimensional line segment.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Extent {
    /// The offset of this extent.
    pub off: u16,
    /// The length of this extent.
    pub len: u16,
}

impl Point {
    /// Shift the point by an offset, avoiding under- or overflow.
    pub fn scroll(&self, x: i16, y: i16) -> Self {
        let nx = if x < 0 {
            self.x.saturating_sub(x.abs() as u16)
        } else {
            self.x.saturating_add(x.abs() as u16)
        };
        let ny = if y < 0 {
            self.y.saturating_sub(y.abs() as u16)
        } else {
            self.y.saturating_add(y.abs() as u16)
        };
        Point { x: nx, y: ny }
    }
    /// Clamp a point, constraining it to fall within `rect`.
    pub fn clamp(&self, rect: Rect) -> Self {
        Point {
            x: self.x.clamp(rect.tl.x, rect.tl.x + rect.w),
            y: self.y.clamp(rect.tl.y, rect.tl.y + rect.h),
        }
    }
    /// Like scroll, but constrained within a rectangle.
    pub fn scroll_within(&self, x: i16, y: i16, rect: Rect) -> Self {
        let nx = if x < 0 {
            self.x.saturating_sub(x.abs() as u16)
        } else {
            self.x.saturating_add(x.abs() as u16)
        };
        let ny = if y < 0 {
            self.y.saturating_sub(y.abs() as u16)
        } else {
            self.y.saturating_add(y.abs() as u16)
        };
        Point { x: nx, y: ny }.clamp(rect)
    }
}

/// A rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Rect {
    /// Top-left corner
    pub tl: Point,
    /// Width
    pub w: u16,
    /// Height
    pub h: u16,
}

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

impl Default for Rect {
    fn default() -> Rect {
        Rect::new(0, 0, 0, 0)
    }
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect {
            tl: Point { x, y },
            w,
            h,
        }
    }
    /// Given a point that falls within this rectangle, rebase the point to be
    /// relative to our origin. If the point falls outside the rect, an error is
    /// returned.
    pub fn rebase(&self, pt: Point) -> Result<Point, CanopyError> {
        if !self.contains_point(pt) {
            return Err(CanopyError::Geometry("co-ords outside rectangle".into()));
        }
        Ok(Point {
            x: pt.x - self.tl.x,
            y: pt.y - self.tl.y,
        })
    }
    /// Does this rectangle contain the point?
    pub fn contains_point(&self, p: Point) -> bool {
        if p.x < self.tl.x || p.x >= self.tl.x + self.w {
            false
        } else {
            !(p.y < self.tl.y || p.y >= self.tl.y + self.h)
        }
    }
    /// A safe function for scrolling the rectangle by an offset, which won't
    /// under- or overflow.
    pub fn scroll(&self, x: i16, y: i16) -> Rect {
        Rect {
            tl: self.tl.scroll(x, y),
            w: self.w,
            h: self.h,
        }
    }
    /// Does this rectangle completely enclose the other?
    pub fn contains_rect(&self, other: Rect) -> bool {
        // The rectangle is completely contained if both the upper left and the
        // lower right points are inside self.
        self.contains_point(other.tl)
            && self.contains_point(Point {
                x: other.tl.x + other.w - 1,
                y: other.tl.y + other.h - 1,
            })
    }
    /// Extracts an inner rectangle, given a border width.
    pub fn inner(&self, border: u16) -> Result<Rect, CanopyError> {
        if self.w < (border * 2) || self.h < (border * 2) {
            return Err(CanopyError::Geometry("rectangle too small".into()));
        }
        Ok(Rect {
            tl: Point {
                x: self.tl.x + border,
                y: self.tl.y + border,
            },
            w: self.w - (border * 2),
            h: self.h - (border * 2),
        })
    }
    /// Extracts a frame for this rect, given a border width. The interior of the frame will match a call to inner() with the same arguments.
    pub fn frame(&self, border: u16) -> Result<Frame, CanopyError> {
        if self.w < (border * 2) || self.h < (border * 2) {
            return Err(CanopyError::Geometry("rectangle too small".into()));
        }
        Ok(Frame {
            top: Rect {
                tl: Point {
                    x: self.tl.x + border,
                    y: self.tl.y,
                },
                w: self.w - 2 * border,
                h: border,
            },
            bottom: Rect {
                tl: Point {
                    x: self.tl.x + border,
                    y: self.tl.y + self.h - border,
                },
                w: self.w - 2 * border,
                h: border,
            },
            left: Rect {
                tl: Point {
                    x: self.tl.x,
                    y: self.tl.y + border,
                },
                w: border,
                h: self.h - 2 * border,
            },
            right: Rect {
                tl: Point {
                    x: self.tl.x + self.w - border,
                    y: self.tl.y + border,
                },
                w: border,
                h: self.h - 2 * border,
            },
            topleft: Rect {
                tl: Point {
                    x: self.tl.x,
                    y: self.tl.y,
                },
                w: border,
                h: border,
            },
            topright: Rect {
                tl: Point {
                    x: self.tl.x + self.w - border,
                    y: self.tl.y,
                },
                w: border,
                h: border,
            },
            bottomleft: Rect {
                tl: Point {
                    x: self.tl.x,
                    y: self.tl.y + self.h - border,
                },
                w: border,
                h: border,
            },
            bottomright: Rect {
                tl: Point {
                    x: self.tl.x + self.w - border,
                    y: self.tl.y + self.h - border,
                },
                w: border,
                h: border,
            },
        })
    }

    /// Scroll this rectangle, constrained to be within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, self unchanged.
    pub fn scroll_within(&self, x: i16, y: i16, rect: Rect) -> Self {
        if rect.w < self.w || rect.h < self.h {
            *self
        } else {
            Rect {
                tl: self.tl.scroll_within(
                    x,
                    y,
                    Rect {
                        tl: rect.tl,
                        h: rect.h.saturating_sub(self.h),
                        w: rect.w.saturating_sub(self.w),
                    },
                ),
                w: self.w,
                h: self.h,
            }
        }
    }

    /// Clamp this rectangle, constraining it lie within another rectangle. The
    /// size of the returned Rect is always equal to that of self. If self is
    /// larger than the enclosing rectangle, return an error.
    pub fn clamp(&self, rect: Rect) -> Result<Self, CanopyError> {
        if rect.w < self.w || rect.h < self.h {
            Err(CanopyError::Geometry(
                "can't clamp to smaller rectangle".into(),
            ))
        } else {
            Ok(Rect {
                tl: self.tl.clamp(Rect {
                    tl: rect.tl,
                    h: rect.h.saturating_sub(self.h),
                    w: rect.w.saturating_sub(self.w),
                }),
                w: self.w,
                h: self.h,
            })
        }
    }

    /// Splits the rectangle horizontally into n sections, as close to equally
    /// sized as possible.
    pub fn split_horizontal(&self, n: u16) -> Result<Vec<Rect>, CanopyError> {
        let widths = split(self.w, n)?;
        let mut off: u16 = self.tl.x;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect {
                tl: Point {
                    x: off,
                    y: self.tl.y,
                },
                w: widths[i as usize],
                h: self.h,
            });
            off += widths[i as usize];
        }
        Ok(ret)
    }
    /// Splits the rectangle vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u16) -> Result<Vec<Rect>, CanopyError> {
        let heights = split(self.h, n)?;
        let mut off: u16 = self.tl.y;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect {
                tl: Point {
                    x: self.tl.x,
                    y: off,
                },
                w: self.w,
                h: heights[i as usize],
            });
            off += heights[i as usize];
        }
        Ok(ret)
    }
    /// Splits the rectangle into columns, with each column split into rows.
    /// Returns a Vec of rects per column.
    pub fn split_panes(&self, spec: Vec<u16>) -> Result<Vec<Vec<Rect>>, CanopyError> {
        let mut ret = vec![];

        let cols = split(self.w, spec.len() as u16)?;
        let mut x = self.tl.x;
        for (ci, width) in cols.iter().enumerate() {
            let mut y = self.tl.y;
            let mut colret = vec![];
            for height in split(self.h, spec[ci])? {
                colret.push(Rect {
                    tl: Point { x, y },
                    w: *width,
                    h: height,
                });
                y += height;
            }
            ret.push(colret);
            x += width;
        }
        Ok(ret)
    }
    // Sweeps upwards from the top of the rectangle.
    pub fn search_up(
        &self,
        f: &mut dyn FnMut(Point) -> Result<bool, CanopyError>,
    ) -> Result<(), CanopyError> {
        'outer: for y in (0..self.tl.y).rev() {
            for x in self.tl.x..(self.tl.x + self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps downwards from the bottom of the rectangle.
    pub fn search_down(
        &self,
        f: &mut dyn FnMut(Point) -> Result<bool, CanopyError>,
    ) -> Result<(), CanopyError> {
        'outer: for y in self.tl.y + self.h..u16::MAX {
            for x in self.tl.x..(self.tl.x + self.w) {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps leftwards the left of the rectangle.
    pub fn search_left(
        &self,
        f: &mut dyn FnMut(Point) -> Result<bool, CanopyError>,
    ) -> Result<(), CanopyError> {
        'outer: for x in (0..self.tl.x).rev() {
            for y in self.tl.y..self.tl.y + self.h {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps rightwards from the right of the rectangle.
    pub fn search_right(
        &self,
        f: &mut dyn FnMut(Point) -> Result<bool, CanopyError>,
    ) -> Result<(), CanopyError> {
        'outer: for x in self.tl.x + self.w..u16::MAX {
            for y in self.tl.y..self.tl.y + self.h {
                if f(Point { x, y })? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps to and fro from the right of the rectangle to the left.
    pub fn search(
        &self,
        dir: Direction,
        f: &mut dyn FnMut(Point) -> Result<bool, CanopyError>,
    ) -> Result<(), CanopyError> {
        match dir {
            Direction::Up => self.search_up(f),
            Direction::Down => self.search_down(f),
            Direction::Left => self.search_left(f),
            Direction::Right => self.search_right(f),
        }
    }

    /// Extract a section of this rect based on an extent.
    pub fn vextract(&self, e: Extent) -> Result<Self, CanopyError> {
        if !self.vextent().contains(e) {
            Err(CanopyError::Geometry(
                "extract extent outside rectangle".into(),
            ))
        } else {
            Ok(Rect {
                tl: Point {
                    x: self.tl.x,
                    y: e.off,
                },
                w: self.w,
                h: e.len,
            })
        }
    }

    /// Extract a horizontal section of this rect based on an extent.
    pub fn hextract(&self, e: Extent) -> Result<Self, CanopyError> {
        if !self.hextent().contains(e) {
            Err(CanopyError::Geometry(
                "extract extent outside rectangle".into(),
            ))
        } else {
            Ok(Rect {
                tl: Point {
                    x: e.off,
                    y: self.tl.y,
                },
                w: e.len,
                h: self.h,
            })
        }
    }

    /// The vertical extent of this rect.
    pub fn vextent(&self) -> Extent {
        Extent {
            off: self.tl.y,
            len: self.h,
        }
    }

    /// The horizontal extent of this rect.
    pub fn hextent(&self) -> Extent {
        Extent {
            off: self.tl.x,
            len: self.w,
        }
    }
}

/// Split a length into n sections, as evenly as possible.
fn split(len: u16, n: u16) -> Result<Vec<u16>, CanopyError> {
    if n == 0 {
        return Err(CanopyError::Geometry("divide by zero".into()));
    }
    let w = len / n;
    let rem = len % n;
    let mut v = Vec::with_capacity(n as usize);
    for i in 0..n {
        v.push(if i < rem { w + 1 } else { w })
    }
    Ok(v)
}

impl Extent {
    /// The far limit of the extent.
    pub fn far(&self) -> u16 {
        self.off + self.len
    }
    /// Does other lie within this extent.
    pub fn contains(&self, other: Extent) -> bool {
        self.off <= other.off && self.far() >= other.far()
    }

    /// Split this extent into (pre, active, post) extents, based on the
    /// position of a window within a view. The main use for this funtion is
    /// computation of the active indicator size and position in a scrollbar.
    pub fn split_active(
        &self,
        window: Extent,
        view: Extent,
    ) -> Result<(Extent, Extent, Extent), CanopyError> {
        if window.len == 0 {
            Err(CanopyError::Geometry("window cannot be zero length".into()))
        } else if !view.contains(window) {
            Err(CanopyError::Geometry(format!(
                "view {:?} does not contain window {:?}",
                view, window,
            )))
        } else {
            // Compute the fraction each section occupies of the view.
            let pref = (window.off - view.off) as f64 / view.len as f64;
            let postf = (view.far() - window.far()) as f64 / view.len as f64;

            // Now compute the true true length in terms of the space
            let pre = (pref * self.len as f64).floor() as u16;
            let post = (postf * self.len as f64).floor() as u16;
            let active = self.len - pre - post;

            Ok((
                Extent {
                    off: self.off,
                    len: pre as u16,
                },
                Extent {
                    off: self.off + pre as u16,
                    len: active,
                },
                Extent {
                    off: self.off + pre + active,
                    len: post,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extent_contains() -> Result<(), CanopyError> {
        let v = Extent { off: 1, len: 3 };
        assert!(v.contains(Extent { off: 1, len: 3 }));
        assert!(!v.contains(Extent { off: 1, len: 4 }));
        assert!(!v.contains(Extent { off: 2, len: 3 }));
        assert!(!v.contains(Extent { off: 0, len: 2 }));

        Ok(())
    }

    #[test]
    fn extent_split_active() -> Result<(), CanopyError> {
        let v = Extent { off: 10, len: 10 };
        assert_eq!(
            v.split_active(Extent { off: 100, len: 50 }, Extent { off: 100, len: 100 })?,
            (
                Extent { off: 10, len: 0 },
                Extent { off: 10, len: 5 },
                Extent { off: 15, len: 5 },
            )
        );
        assert_eq!(
            v.split_active(Extent { off: 150, len: 50 }, Extent { off: 100, len: 100 })?,
            (
                Extent { off: 10, len: 5 },
                Extent { off: 15, len: 5 },
                Extent { off: 20, len: 0 },
            )
        );
        assert_eq!(
            v.split_active(Extent { off: 130, len: 40 }, Extent { off: 100, len: 100 })?,
            (
                Extent { off: 10, len: 3 },
                Extent { off: 13, len: 4 },
                Extent { off: 17, len: 3 },
            )
        );
        assert_eq!(
            v.split_active(Extent { off: 100, len: 100 }, Extent { off: 100, len: 100 })?,
            (
                Extent { off: 10, len: 0 },
                Extent { off: 10, len: 10 },
                Extent { off: 20, len: 0 },
            )
        );
        Ok(())
    }

    #[test]
    fn tsearch() -> Result<(), CanopyError> {
        let bounds = Rect {
            tl: Point { x: 0, y: 0 },
            w: 6,
            h: 6,
        };
        let r = Rect {
            tl: Point { x: 2, y: 2 },
            w: 2,
            h: 2,
        };

        let mut v: Vec<Point> = vec![];
        r.search_up(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 2, y: 1 },
                Point { x: 3, y: 1 },
                Point { x: 2, y: 0 },
                Point { x: 3, y: 0 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_left(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 1, y: 2 },
                Point { x: 1, y: 3 },
                Point { x: 0, y: 2 },
                Point { x: 0, y: 3 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_down(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 2, y: 4 },
                Point { x: 3, y: 4 },
                Point { x: 2, y: 5 },
                Point { x: 3, y: 5 }
            ]
        );

        let mut v: Vec<Point> = vec![];
        r.search_right(&mut |p| {
            Ok(if !bounds.contains_point(p) {
                true
            } else {
                v.push(p);
                false
            })
        })?;
        assert_eq!(
            v,
            [
                Point { x: 4, y: 2 },
                Point { x: 4, y: 3 },
                Point { x: 5, y: 2 },
                Point { x: 5, y: 3 }
            ]
        );

        Ok(())
    }
    #[test]
    fn inner() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 0, y: 0 },
            w: 10,
            h: 10,
        };
        assert_eq!(
            r.inner(1)?,
            Rect {
                tl: Point { x: 1, y: 1 },
                w: 8,
                h: 8,
            },
        );
        Ok(())
    }
    #[test]
    fn contains() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 10,
            h: 10,
        };
        assert!(r.contains_point(Point { x: 10, y: 10 }));
        assert!(!r.contains_point(Point { x: 9, y: 10 }));
        assert!(!r.contains_point(Point { x: 20, y: 20 }));
        assert!(r.contains_point(Point { x: 19, y: 19 }));
        assert!(!r.contains_point(Point { x: 20, y: 21 }));

        assert!(r.contains_rect(Rect {
            tl: Point { x: 10, y: 10 },
            w: 1,
            h: 1
        }));
        assert!(r.contains_rect(r));

        Ok(())
    }

    #[test]
    fn tsplit() -> Result<(), CanopyError> {
        assert_eq!(split(7, 3)?, vec![3, 2, 2]);
        assert_eq!(split(6, 3)?, vec![2, 2, 2]);
        assert_eq!(split(9, 1)?, vec![9]);
        Ok(())
    }

    #[test]
    fn trebase() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 10,
            h: 10,
        };
        assert_eq!(r.rebase(Point { x: 11, y: 11 })?, Point { x: 1, y: 1 });
        assert_eq!(r.rebase(Point { x: 10, y: 10 })?, Point { x: 0, y: 0 });

        if let Ok(_) = r.rebase(Point { x: 9, y: 9 }) {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn tscroll() -> Result<(), CanopyError> {
        assert_eq!(
            Rect {
                tl: Point { x: 5, y: 5 },
                w: 10,
                h: 10,
            }
            .scroll(-10, -10),
            Rect {
                tl: Point { x: 0, y: 0 },
                w: 10,
                h: 10
            }
        );
        assert_eq!(
            Rect {
                tl: Point {
                    x: u16::MAX - 5,
                    y: u16::MAX - 5,
                },
                w: 10,
                h: 10,
            }
            .scroll(10, 10),
            Rect {
                tl: Point {
                    x: u16::MAX,
                    y: u16::MAX,
                },
                w: 10,
                h: 10
            }
        );
        Ok(())
    }

    #[test]
    fn tframe() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 10,
            h: 10,
        };
        assert_eq!(
            r.frame(1)?,
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
    #[test]
    fn trect_clamp() -> Result<(), CanopyError> {
        assert_eq!(
            Rect {
                tl: Point { x: 11, y: 11 },
                w: 5,
                h: 5,
            }
            .clamp(Rect {
                tl: Point { x: 10, y: 10 },
                w: 10,
                h: 10,
            })?,
            Rect {
                tl: Point { x: 11, y: 11 },
                w: 5,
                h: 5,
            },
        );
        assert_eq!(
            Rect {
                tl: Point { x: 19, y: 19 },
                w: 5,
                h: 5,
            }
            .clamp(Rect {
                tl: Point { x: 10, y: 10 },
                w: 10,
                h: 10,
            })?,
            Rect {
                tl: Point { x: 15, y: 15 },
                w: 5,
                h: 5,
            },
        );
        assert_eq!(
            Rect {
                tl: Point { x: 5, y: 5 },
                w: 5,
                h: 5,
            }
            .clamp(Rect {
                tl: Point { x: 10, y: 10 },
                w: 10,
                h: 10,
            })?,
            Rect {
                tl: Point { x: 10, y: 10 },
                w: 5,
                h: 5,
            },
        );
        Ok(())
    }

    #[test]
    fn trect_scroll_within() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 5,
            h: 5,
        };
        assert_eq!(
            Rect {
                tl: Point { x: 11, y: 11 },
                w: 5,
                h: 5,
            },
            r.scroll_within(
                1,
                1,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 10,
                    h: 10,
                },
            )
        );
        assert_eq!(
            Rect {
                tl: Point { x: 15, y: 15 },
                w: 5,
                h: 5,
            },
            r.scroll_within(
                10,
                10,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 10,
                    h: 10,
                },
            )
        );
        // Degenerate case - trying to scroll within a smaller rect.
        assert_eq!(
            r.scroll_within(
                1,
                1,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 2,
                    h: 2,
                },
            ),
            r
        );
        Ok(())
    }

    #[test]
    fn tpoint_scroll_within() -> Result<(), CanopyError> {
        let p = Point { x: 15, y: 15 };
        assert_eq!(
            Point { x: 10, y: 10 },
            p.scroll_within(
                -10,
                -10,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 10,
                    h: 10,
                },
            )
        );
        assert_eq!(
            Point { x: 20, y: 20 },
            p.scroll_within(
                10,
                10,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 10,
                    h: 10,
                },
            )
        );
        assert_eq!(
            Point { x: 16, y: 15 },
            p.scroll_within(
                1,
                0,
                Rect {
                    tl: Point { x: 10, y: 10 },
                    w: 10,
                    h: 10,
                },
            )
        );
        Ok(())
    }

    #[test]
    fn tsplit_panes() -> Result<(), CanopyError> {
        let r = Rect {
            tl: Point { x: 10, y: 10 },
            w: 40,
            h: 40,
        };
        assert_eq!(
            r.split_panes(vec![2, 2])?,
            vec![
                [
                    Rect {
                        tl: Point { x: 10, y: 10 },
                        w: 20,
                        h: 20
                    },
                    Rect {
                        tl: Point { x: 10, y: 30 },
                        w: 20,
                        h: 20
                    }
                ],
                [
                    Rect {
                        tl: Point { x: 30, y: 10 },
                        w: 20,
                        h: 20
                    },
                    Rect {
                        tl: Point { x: 30, y: 30 },
                        w: 20,
                        h: 20
                    }
                ]
            ],
        );
        assert_eq!(
            r.split_panes(vec![2, 1])?,
            vec![
                vec![
                    Rect {
                        tl: Point { x: 10, y: 10 },
                        w: 20,
                        h: 20
                    },
                    Rect {
                        tl: Point { x: 10, y: 30 },
                        w: 20,
                        h: 20
                    }
                ],
                vec![Rect {
                    tl: Point { x: 30, y: 10 },
                    w: 20,
                    h: 40
                }],
            ],
        );
        Ok(())
    }
}
