use anyhow::{format_err, Result};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// A rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// A frame extracted from a rectangle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Frame {
    // Spans the entire width, including the top left and right corners.
    pub top: Rect,
    // Spans the entire width, including the bottom left and right corners.
    pub bottom: Rect,
    pub left: Rect,
    pub right: Rect,
}

impl Default for Rect {
    fn default() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}

impl Rect {
    /// Rebase co-ordinates to be relative to the origin of this rect. If the
    /// points are outside the rect, an error is returned.
    pub fn rebase(&self, x: u16, y: u16) -> Result<(u16, u16)> {
        if !self.contains(x, y) {
            return Err(anyhow::format_err!("co-ords outside rectangle"));
        }
        Ok((x - self.x, y - self.y))
    }
    /// Does this rectangle contain the point?
    pub fn contains(&self, x: u16, y: u16) -> bool {
        if x < self.x || x >= self.x + self.width {
            false
        } else {
            !(y < self.y || y >= self.y + self.height)
        }
    }
    /// Extracts an inner rectangle, given a border width.
    pub fn inner(&self, border: u16) -> Result<Rect> {
        if self.width < (border * 2) || self.height < (border * 2) {
            return Err(anyhow::format_err!("rectangle too small"));
        }
        Ok(Rect {
            x: self.x + border,
            y: self.y + border,
            width: self.width - (border * 2),
            height: self.height - (border * 2),
        })
    }
    /// Extracts a frame for this rect, given a border width. The interior of the frame will match a call to inner() with the same arguments.
    pub fn frame(&self, border: u16) -> Result<Frame> {
        if self.width < (border * 2) || self.height < (border * 2) {
            return Err(anyhow::format_err!("rectangle too small"));
        }
        Ok(Frame {
            top: Rect {
                x: self.x,
                y: self.y,
                width: self.width,
                height: border,
            },
            bottom: Rect {
                x: self.x,
                y: self.y + self.height - border,
                width: self.width,
                height: border,
            },
            left: Rect {
                x: self.x,
                y: self.y + border,
                width: border,
                height: self.height - 2 * border,
            },
            right: Rect {
                x: self.x + self.width - border,
                y: self.y + border,
                width: border,
                height: self.height - 2 * border,
            },
        })
    }
    /// Splits the rectangle horizontally into n sections, as close to equally
    /// sized as possible.
    pub fn split_horizontal(&self, n: u16) -> Result<Vec<Rect>> {
        let widths = split(self.width, n)?;
        let mut off: u16 = self.x;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect {
                x: off,
                y: self.y,
                width: widths[i as usize],
                height: self.height,
            });
            off += widths[i as usize];
        }
        Ok(ret)
    }
    /// Splits the rectangle vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u16) -> Result<Vec<Rect>> {
        let heights = split(self.height, n)?;
        let mut off: u16 = self.y;
        let mut ret = vec![];
        for i in 0..n {
            ret.push(Rect {
                x: self.x,
                y: off,
                width: self.width,
                height: heights[i as usize],
            });
            off += heights[i as usize];
        }
        Ok(ret)
    }

    /// Splits the rectangle into columns, with each column split into rows.
    /// Returns a Vec of rects per column.
    pub fn split_panes(&self, spec: Vec<u16>) -> Result<Vec<Vec<Rect>>> {
        let mut ret = vec![];

        let cols = split(self.width, spec.len() as u16)?;
        let mut x = self.x;
        for (ci, width) in cols.iter().enumerate() {
            let mut y = self.y;
            let mut colret = vec![];
            for height in split(self.height, spec[ci])? {
                colret.push(Rect {
                    x,
                    y,
                    width: *width,
                    height,
                });
                y += height;
            }
            ret.push(colret);
            x += width;
        }
        Ok(ret)
    }
    // Sweeps upwards from the top of the rectangle.
    pub fn search_up(&self, f: &mut dyn FnMut(u16, u16) -> Result<bool>) -> Result<()> {
        'outer: for y in (0..self.y).rev() {
            for x in self.x..(self.x + self.width) {
                if f(x, y)? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps downwards from the bottom of the rectangle.
    pub fn search_down(&self, f: &mut dyn FnMut(u16, u16) -> Result<bool>) -> Result<()> {
        'outer: for y in self.y + self.height..u16::MAX {
            for x in self.x..(self.x + self.width) {
                if f(x, y)? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps leftwards the left of the rectangle.
    pub fn search_left(&self, f: &mut dyn FnMut(u16, u16) -> Result<bool>) -> Result<()> {
        'outer: for x in (0..self.x).rev() {
            for y in self.y..self.y + self.height {
                if f(x, y)? {
                    break 'outer;
                }
            }
        }
        Ok(())
    }
    // Sweeps rightwards from the right of the rectangle.
    pub fn search_right(&self, f: &mut dyn FnMut(u16, u16) -> Result<bool>) -> Result<()> {
        'outer: for x in self.x + self.width..u16::MAX {
            for y in self.y..self.y + self.height {
                if f(x, y)? {
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
        f: &mut dyn FnMut(u16, u16) -> Result<bool>,
    ) -> Result<()> {
        match dir {
            Direction::Up => self.search_up(f),
            Direction::Down => self.search_down(f),
            Direction::Left => self.search_left(f),
            Direction::Right => self.search_right(f),
        }
    }
}

/// Split a length into n sections, as evenly as possible.
fn split(len: u16, n: u16) -> Result<Vec<u16>> {
    if n == 0 {
        return Err(format_err!("divide by zero"));
    }
    let w = len / n;
    let rem = len % n;
    let mut v = Vec::with_capacity(n as usize);
    for i in 0..n {
        v.push(if i < rem { w + 1 } else { w })
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tsearch() -> Result<()> {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 6,
            height: 6,
        };
        let r = Rect {
            x: 2,
            y: 2,
            width: 2,
            height: 2,
        };

        let mut v: Vec<(u16, u16)> = vec![];
        r.search_up(&mut |x, y| {
            Ok(if !bounds.contains(x, y) {
                true
            } else {
                v.push((x, y));
                false
            })
        })?;
        assert_eq!(v, [(2, 1), (3, 1), (2, 0), (3, 0)]);

        let mut v: Vec<(u16, u16)> = vec![];
        r.search_left(&mut |x, y| {
            Ok(if !bounds.contains(x, y) {
                true
            } else {
                v.push((x, y));
                false
            })
        })?;
        assert_eq!(v, [(1, 2), (1, 3), (0, 2), (0, 3)]);

        let mut v: Vec<(u16, u16)> = vec![];
        r.search_down(&mut |x, y| {
            Ok(if !bounds.contains(x, y) {
                true
            } else {
                v.push((x, y));
                false
            })
        })?;
        assert_eq!(v, [(2, 4), (3, 4), (2, 5), (3, 5)]);

        let mut v: Vec<(u16, u16)> = vec![];
        r.search_right(&mut |x, y| {
            Ok(if !bounds.contains(x, y) {
                true
            } else {
                v.push((x, y));
                false
            })
        })?;
        assert_eq!(v, [(4, 2), (4, 3), (5, 2), (5, 3)]);

        Ok(())
    }
    #[test]
    fn inner() -> Result<()> {
        let r = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        assert_eq!(
            r.inner(1)?,
            Rect {
                x: 1,
                y: 1,
                width: 8,
                height: 8,
            },
        );
        Ok(())
    }
    #[test]
    fn contains() -> Result<()> {
        let r = Rect {
            x: 10,
            y: 10,
            width: 10,
            height: 10,
        };
        assert!(r.contains(10, 10));
        assert!(!r.contains(9, 10));
        assert!(!r.contains(20, 20));
        assert!(r.contains(19, 19));
        assert!(!r.contains(20, 21));
        Ok(())
    }

    #[test]
    fn tsplit() -> Result<()> {
        assert_eq!(split(7, 3)?, vec![3, 2, 2]);
        assert_eq!(split(6, 3)?, vec![2, 2, 2]);
        assert_eq!(split(9, 1)?, vec![9]);
        Ok(())
    }

    #[test]
    fn trebase() -> Result<()> {
        let r = Rect {
            x: 10,
            y: 10,
            width: 10,
            height: 10,
        };
        assert_eq!(r.rebase(11, 11)?, (1, 1));
        assert_eq!(r.rebase(10, 10)?, (0, 0));

        if let Ok(_) = r.rebase(9, 9) {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn tframe() -> Result<()> {
        let r = Rect {
            x: 10,
            y: 10,
            width: 10,
            height: 10,
        };
        assert_eq!(
            r.frame(1)?,
            Frame {
                top: Rect {
                    x: 10,
                    y: 10,
                    width: 10,
                    height: 1
                },
                bottom: Rect {
                    x: 10,
                    y: 19,
                    width: 10,
                    height: 1
                },
                left: Rect {
                    x: 10,
                    y: 11,
                    width: 1,
                    height: 8
                },
                right: Rect {
                    x: 19,
                    y: 11,
                    width: 1,
                    height: 8
                },
            }
        );
        Ok(())
    }

    #[test]
    fn tsplit_panes() -> Result<()> {
        let r = Rect {
            x: 10,
            y: 10,
            width: 40,
            height: 40,
        };
        assert_eq!(
            r.split_panes(vec![2, 2])?,
            vec![
                [
                    Rect {
                        x: 10,
                        y: 10,
                        width: 20,
                        height: 20
                    },
                    Rect {
                        x: 10,
                        y: 30,
                        width: 20,
                        height: 20
                    }
                ],
                [
                    Rect {
                        x: 30,
                        y: 10,
                        width: 20,
                        height: 20
                    },
                    Rect {
                        x: 30,
                        y: 30,
                        width: 20,
                        height: 20
                    }
                ]
            ],
        );
        assert_eq!(
            r.split_panes(vec![2, 1])?,
            vec![
                vec![
                    Rect {
                        x: 10,
                        y: 10,
                        width: 20,
                        height: 20
                    },
                    Rect {
                        x: 10,
                        y: 30,
                        width: 20,
                        height: 20
                    }
                ],
                vec![Rect {
                    x: 30,
                    y: 10,
                    width: 20,
                    height: 40
                }],
            ],
        );
        Ok(())
    }
}
