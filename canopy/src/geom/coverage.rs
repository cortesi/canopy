use bitvec::prelude::*;

use super::{Expanse, Line, Point, Rect};

/// Coverage accumulates `Rect`s that have been drawn and calculates which
/// portions of an enclosing area remain un-covered. This allows us to
/// calculate which areas of a Node screen rectangle remain need to be cleared
/// after a render sweep.
#[derive(Debug, Default)]
pub struct Coverage {
    area: Expanse,
    cells: BitVec,
}

impl Coverage {
    pub fn new(area: Expanse) -> Self {
        Coverage {
            area,
            cells: BitVec::repeat(false, area.area() as usize),
        }
    }

    /// Add a rectangle to the cover set. Rects and portions of rects that fall
    /// outside of the area we're initialized with are ignored.
    pub fn add(&mut self, x: Rect) {
        if let Some(r) = x.intersect(&self.area.rect()) {
            for i in 0..r.h {
                let off = (r.tl.y + i) * self.area.w + r.tl.x;
                self.cells[off as usize..(off + r.w) as usize].fill(true);
            }
        }
    }

    /// Return a set of `Line`s that represent the un-covered section of the
    /// input rectangle. Lines are emitted from top to bottom then left to right
    /// to allow them to be efficiently drawn. This method consumes the Coverage
    /// object.
    pub fn uncovered(&self) -> Vec<Line> {
        let mut ret = vec![];
        let mut current: Option<Line> = None;
        for i in self.cells.iter_zeros() {
            let x = (i % self.area.w as usize) as u16;
            let y = (i / self.area.w as usize) as u16;

            if let Some(mut c) = current.as_mut() {
                if y != c.tl.y || x != c.tl.x + c.w {
                    ret.push(*c);
                    current = Some(Line {
                        tl: Point { x, y },
                        w: 1,
                    });
                } else {
                    c.w += 1;
                }
            } else {
                current = Some(Line {
                    tl: Point { x, y },
                    w: 1,
                });
            }
        }
        if let Some(c) = current {
            ret.push(c);
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    #[test]
    fn coverage() -> Result<()> {
        let c = Coverage::new(Expanse::new(10, 3));
        assert_eq!(
            c.uncovered(),
            [
                Line {
                    tl: Point { x: 0, y: 0 },
                    w: 10
                },
                Line {
                    tl: Point { x: 0, y: 1 },
                    w: 10
                },
                Line {
                    tl: Point { x: 0, y: 2 },
                    w: 10
                }
            ]
        );

        let mut c = Coverage::new(Expanse::new(4, 4));
        c.add(Rect::new(1, 1, 2, 2));
        assert_eq!(
            c.uncovered(),
            [
                Line {
                    tl: Point { x: 0, y: 0 },
                    w: 4
                },
                Line {
                    tl: Point { x: 0, y: 1 },
                    w: 1
                },
                Line {
                    tl: Point { x: 3, y: 1 },
                    w: 1
                },
                Line {
                    tl: Point { x: 0, y: 2 },
                    w: 1
                },
                Line {
                    tl: Point { x: 3, y: 2 },
                    w: 1
                },
                Line {
                    tl: Point { x: 0, y: 3 },
                    w: 4
                }
            ]
        );

        let mut c = Coverage::new(Expanse::new(4, 4));
        c.add(Rect::new(0, 0, 2, 2));
        c.add(Rect::new(2, 2, 20, 20));
        assert_eq!(
            c.uncovered(),
            [
                Line {
                    tl: Point { x: 2, y: 0 },
                    w: 2
                },
                Line {
                    tl: Point { x: 2, y: 1 },
                    w: 2
                },
                Line {
                    tl: Point { x: 0, y: 2 },
                    w: 2
                },
                Line {
                    tl: Point { x: 0, y: 3 },
                    w: 2
                }
            ]
        );

        Ok(())
    }
}
