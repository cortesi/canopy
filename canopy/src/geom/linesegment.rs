use crate::{Error, Result};

/// An exctent is a directionless one-dimensional line segment.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct LineSegment {
    /// The offset of this extent.
    pub off: u16,
    /// The length of this extent.
    pub len: u16,
}

impl LineSegment {
    /// The far limit of the extent.
    pub fn far(&self) -> u16 {
        self.off + self.len
    }

    /// Does other lie within this extent.
    pub fn contains(&self, other: &LineSegment) -> bool {
        self.off <= other.off && self.far() >= other.far()
    }

    /// Return the intersection between this line segment and other. The line
    /// segment returned will always have a non-zero length.
    pub fn intersect(&self, other: &LineSegment) -> Option<LineSegment> {
        if self.len == 0 || other.len == 0 {
            None
        } else if self.contains(other) {
            Some(*other)
        } else if other.contains(self) {
            Some(*self)
        } else if self.off <= other.off && other.off < self.far() {
            Some(LineSegment {
                off: other.off,
                len: self.far() - other.off,
            })
        } else if other.off <= self.off && self.off < other.far() {
            Some(LineSegment {
                off: self.off,
                len: other.far() - self.off,
            })
        } else {
            None
        }
    }

    /// Split this extent into (pre, active, post) extents, based on the
    /// position of a window within a view. The main use for this funtion is
    /// computation of the active indicator size and position in a scrollbar.
    pub fn split_active(
        &self,
        window: LineSegment,
        view: LineSegment,
    ) -> Result<(LineSegment, LineSegment, LineSegment)> {
        if window.len == 0 {
            Err(Error::Geometry("window cannot be zero length".into()))
        } else if !view.contains(&window) {
            Err(Error::Geometry(format!(
                "view {:?} does not contain window {:?}",
                view, window,
            )))
        } else {
            // Compute the fraction each section occupies of the view.
            let pref = (window.off - view.off) as f64 / view.len as f64;
            let postf = (view.far() - window.far()) as f64 / view.len as f64;
            let lenf = self.len as f64;

            // Now compute the true true length in terms of the space. It's
            // important for the active portion to remain the same length
            // regardless of position in the face of rounding, so we compute it
            // first, then compute the other values in terms of it.
            let active = (lenf - (pref * lenf) - (postf * lenf)).ceil();
            let pre = (pref * self.len as f64).floor();
            let post = lenf - active - pre;

            Ok((
                LineSegment {
                    off: self.off,
                    len: pre as u16,
                },
                LineSegment {
                    off: self.off + pre as u16,
                    len: active as u16,
                },
                LineSegment {
                    off: self.off + pre as u16 + active as u16,
                    len: post as u16,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect() -> Result<()> {
        let l = LineSegment { off: 5, len: 5 };

        assert_eq!(
            l.intersect(&LineSegment { off: 6, len: 2 }),
            Some(LineSegment { off: 6, len: 2 })
        );
        assert_eq!(l.intersect(&LineSegment { off: 1, len: 10 }), Some(l));
        assert_eq!(
            l.intersect(&LineSegment { off: 6, len: 8 }),
            Some(LineSegment { off: 6, len: 4 })
        );
        assert_eq!(
            l.intersect(&LineSegment { off: 0, len: 8 }),
            Some(LineSegment { off: 5, len: 3 })
        );
        assert_eq!(l.intersect(&l), Some(l));
        assert_eq!(l.intersect(&LineSegment { off: 0, len: 2 }), None);
        assert_eq!(l.intersect(&LineSegment { off: 10, len: 2 }), None);
        assert_eq!(l.intersect(&LineSegment { off: 5, len: 0 }), None);
        assert_eq!(l.intersect(&LineSegment { off: 0, len: 5 }), None);
        Ok(())
    }

    #[test]
    fn contains() -> Result<()> {
        let v = LineSegment { off: 1, len: 3 };
        assert!(v.contains(&LineSegment { off: 1, len: 3 }));
        assert!(!v.contains(&LineSegment { off: 1, len: 4 }));
        assert!(!v.contains(&LineSegment { off: 2, len: 3 }));
        assert!(!v.contains(&LineSegment { off: 0, len: 2 }));

        Ok(())
    }

    #[test]
    fn split_active() -> Result<()> {
        let v = LineSegment { off: 10, len: 10 };
        assert_eq!(
            v.split_active(
                LineSegment { off: 100, len: 50 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 0 },
                LineSegment { off: 10, len: 5 },
                LineSegment { off: 15, len: 5 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 150, len: 50 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 5 },
                LineSegment { off: 15, len: 5 },
                LineSegment { off: 20, len: 0 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 130, len: 40 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 3 },
                LineSegment { off: 13, len: 4 },
                LineSegment { off: 17, len: 3 },
            )
        );
        assert_eq!(
            v.split_active(
                LineSegment { off: 100, len: 100 },
                LineSegment { off: 100, len: 100 }
            )?,
            (
                LineSegment { off: 10, len: 0 },
                LineSegment { off: 10, len: 10 },
                LineSegment { off: 20, len: 0 },
            )
        );
        Ok(())
    }
}
