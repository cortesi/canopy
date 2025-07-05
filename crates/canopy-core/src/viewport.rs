use crate::Result;
use crate::geom::{Expanse, Point, Rect};

/// A ViewPort manages the size of a node and its projection onto the screen. In many ways, this is
/// the core of Canopy, and the viewport structure and its constraints determines many aspects of
/// Canopy's layout and rendering behavior.
///
/// A Canopy app is a tree of nested ViewPorts, with each maintaining a set of constraints with
/// reference to its parent and children.
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ViewPort {
    /// The location of the node's view rectangle in the parent's canvas. Must only be changed by
    /// the parent node. The space occupied by node is defined by its position and its view
    /// rectangle.
    ///
    /// CONSTRAINT: The position must be within the PARENT's canvas rectangle.
    ///
    /// It is NOT a constraint that the mapped view rectangle must be fully within the parent's
    /// canvas. It may be that the view is larger than the parent's canvas, in which case it will
    /// be clipped.
    position: Point,

    /// The portion of this node that is displayed - a sub-rectangle of the canvas. Must only be
    /// changed by the node itself. This is the portion of the node that is drawn to the screen. To
    /// ease widget implementation, when attempting to draw to the screen any draw operations outside the
    /// screen rectangle are ignored.
    ///
    /// CONSTRAINT: The view rectangle must be fully contained within OUR canvas rectangle.
    view: Rect,

    /// The canvas on which children are positioned, and to which rendering occurs. Must only be
    /// changed by the node itself. You can think of this as a rectangle with co-ordinates (0, 0),
    /// which describes the full size of this node and its children.
    canvas: Expanse,
}

impl ViewPort {
    /// Create a new View with the given outer and inner rectangles. The view
    /// rectangle must be fully contained within the outer rectangle.
    pub fn new(
        canvas: impl Into<Expanse>,
        view: impl Into<Rect>,
        position: impl Into<Point>,
    ) -> Result<ViewPort> {
        let view = view.into();
        let size = canvas.into();
        Ok(ViewPort {
            canvas: size,
            view,
            position: position.into(),
        })
    }

    /// Position of this ViewPort's view within the parent canvas.
    pub fn position(&self) -> Point {
        self.position
    }

    /// This viewport's view rectangle, relative to our own canvas.
    pub fn view(&self) -> Rect {
        self.view
    }

    /// The canvas size for this viewport.
    pub fn canvas(&self) -> Expanse {
        self.canvas
    }

    /// Set the viewport position. The caller must provide the parent's position
    /// and canvas rectangle so that we can verify the new position stays within
    /// bounds.
    pub fn set_position(&mut self, p: Point) {
        self.position = p;
    }

    /// Update the canvas size for this viewport, clamping the current view to
    /// remain within the new canvas.
    pub fn set_canvas(&mut self, sz: Expanse) {
        self.canvas = sz;
        self.view = match self.view.clamp_within(self.canvas.rect()) {
            Ok(v) => v,
            Err(_) => self.canvas.rect(),
        };
    }

    /// Set the visible view rectangle, clamped so that it always falls within
    /// the current canvas.
    pub fn set_view(&mut self, view: Rect) {
        self.view = match view.clamp_within(self.canvas.rect()) {
            Ok(v) => v,
            Err(_) => self.canvas.rect(),
        };
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub(crate) fn scroll_to(&mut self, x: u32, y: u32) {
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        self.view = r.clamp_within(self.canvas.rect()).unwrap();
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub(crate) fn scroll_by(&mut self, x: i32, y: i32) {
        self.view = self.view.shift_within(x, y, self.canvas.rect());
    }

    /// Scroll the view up by the height of the view rectangle.
    pub(crate) fn page_up(&mut self) {
        self.scroll_by(0, -(self.view.h as i32))
    }

    /// Scroll the view down by the height of the view rectangle.
    pub(crate) fn page_down(&mut self) {
        self.scroll_by(0, self.view.h as i32)
    }

    /// Scroll the view up by one line.
    pub(crate) fn scroll_up(&mut self) {
        self.scroll_by(0, -1)
    }

    /// Scroll the view down by one line.
    pub(crate) fn scroll_down(&mut self) {
        self.scroll_by(0, 1)
    }

    /// Scroll the view left by one line.
    pub(crate) fn scroll_left(&mut self) {
        self.scroll_by(-1, 0)
    }

    /// Scroll the view right by one line.
    pub fn scroll_right(&mut self) {
        self.scroll_by(1, 0)
    }

    /// Absolute rectangle for the screen region the node is being projected onto.
    pub fn screen_rect(&self) -> Rect {
        self.view.at(self.position)
    }

    /// Set the node size and the target view size at the same time. We try to retain the old view position, but shift
    /// and resize it to be within the view if necessary.
    pub fn fit_size(&mut self, size: Expanse, view_size: Expanse) {
        let w = size.w.min(view_size.w);
        let h = size.h.min(view_size.h);
        self.canvas = size;
        // Now we just clamp the rect into the view.
        self.view = Rect {
            tl: self.view.tl,
            w,
            h,
        }
        .clamp_within(self.canvas.rect())
        // Safe to unwrap because of w, h computation above.
        .unwrap();
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a vertical
    /// scroll bar for this viewport in the specified margin rect (usually a
    /// right or left vertical margin). Returns None if no scroll bar is needed.
    pub fn vactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        if self.view.h == self.canvas.h {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .vextent()
                .split_active(self.view.vextent(), self.canvas.rect().vextent())?;
            Ok(Some((
                margin.vslice(&pre)?,
                margin.vslice(&active)?,
                margin.vslice(&post)?,
            )))
        }
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a
    /// horizontal scroll bar for this viewport in the specified margin rect
    /// (usually a bottom horizontal margin). Returns None if no scroll bar is
    /// needed.
    pub fn hactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        if self.view.w == self.canvas.w {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .hextent()
                .split_active(self.view.hextent(), self.canvas.rect().hextent())?;
            Ok(Some((
                margin.hslice(&pre)?,
                margin.hslice(&active)?,
                margin.hslice(&post)?,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_size() -> Result<()> {
        let mut v = ViewPort::new((100, 100), (50, 50, 10, 10), (50, 50))?;

        v.fit_size(Expanse::new(50, 50), Expanse::new(20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.fit_size(Expanse::new(100, 100), Expanse::new(20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.fit_size(Expanse::new(10, 10), Expanse::new(10, 10));
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        v.fit_size(Expanse::new(20, 20), Expanse::new(20, 20));
        assert_eq!(v.view, Rect::new(0, 0, 20, 20));

        Ok(())
    }

    #[test]
    fn view_movement() -> Result<()> {
        fn tv<T>(vp: &ViewPort, f: &dyn Fn(&mut ViewPort), r: T)
        where
            T: Into<Rect>,
        {
            let mut v = *vp;
            f(&mut v);
            let r = r.into();
            assert_eq!(v.view, r);
        }

        let v = ViewPort::new((100, 100), (0, 0, 10, 10), (0, 0))?;

        tv(&v, &|v| v.scroll_by(10, 10), (10, 10, 10, 10));
        tv(&v, &|v| v.scroll_by(-20, -20), (0, 0, 10, 10));
        tv(&v, &|v| v.page_down(), (0, 10, 10, 10));
        tv(&v, &|v| v.page_up(), (0, 0, 10, 10));
        tv(&v, &|v| v.scroll_to(50, 50), (50, 50, 10, 10));
        tv(&v, &|v| v.scroll_to(150, 150), (90, 90, 10, 10));

        Ok(())
    }
}
