use super::{Point, Rect};

/// A horizontal line, one character high - essentially a Rect with height 1.
#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub struct Line {
    /// Top-left point for the line.
    pub tl: Point,
    /// Width in cells.
    pub w: u32,
}

impl Line {
    /// Construct a line from coordinates and width.
    pub fn new(x: u32, y: u32, w: u32) -> Self {
        Self {
            tl: Point { x, y },
            w,
        }
    }
    /// Convert the line into a rectangle of height 1.
    pub fn rect(&self) -> Rect {
        Rect {
            tl: self.tl,
            w: self.w,
            h: 1,
        }
    }

    /// Return this line indented from its start, giving up the columns it
    /// skips.
    ///
    /// Moving a line's start without shrinking its width is the mistake this
    /// exists to prevent. A line that keeps its width runs past the extent it
    /// was cut from, and painting it writes over whatever lies beyond, such as
    /// the border it was meant to sit inside. Indenting holds the far edge
    /// still, and indenting past that edge leaves an empty line rather than
    /// wrapping around.
    pub fn indent(&self, columns: u32) -> Self {
        Self {
            tl: Point {
                x: self.tl.x.saturating_add(columns),
                y: self.tl.y,
            },
            w: self.w.saturating_sub(columns),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indenting_holds_the_far_edge_still() {
        let line = Line::new(10, 5, 8);
        assert_eq!(line.indent(0), line);
        assert_eq!(line.indent(2), Line::new(12, 5, 6));

        // The end of the line never moves, so an indented line cannot overrun
        // the extent it came from.
        for columns in 0..=8 {
            let indented = line.indent(columns);
            assert_eq!(indented.tl.x + indented.w, line.tl.x + line.w);
        }

        // Indenting to or past the end leaves nothing to paint.
        assert_eq!(line.indent(8), Line::new(18, 5, 0));
        assert_eq!(line.indent(99).w, 0);
    }
}
