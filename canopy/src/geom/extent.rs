use crate::{Error, Result};

/// An exctent is a directionless one-dimensional line segment.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Extent {
    /// The offset of this extent.
    pub off: u16,
    /// The length of this extent.
    pub len: u16,
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
    pub fn split_active(&self, window: Extent, view: Extent) -> Result<(Extent, Extent, Extent)> {
        if window.len == 0 {
            Err(Error::Geometry("window cannot be zero length".into()))
        } else if !view.contains(window) {
            Err(Error::Geometry(format!(
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
    fn extent_contains() -> Result<()> {
        let v = Extent { off: 1, len: 3 };
        assert!(v.contains(Extent { off: 1, len: 3 }));
        assert!(!v.contains(Extent { off: 1, len: 4 }));
        assert!(!v.contains(Extent { off: 2, len: 3 }));
        assert!(!v.contains(Extent { off: 0, len: 2 }));

        Ok(())
    }

    #[test]
    fn extent_split_active() -> Result<()> {
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
}
