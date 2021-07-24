use super::{Point, Rect, Size};
use crate::error;
use crate::Result;

/// ViewPort manages three rectangles in concert: `outer` is the total virtual
/// size of the node, `view` is some sub-rectangle of `outer`. The `screen`, is
/// a rectangle on the physical screen that this node paints to. It is larger
/// than or equal to view.
///
/// The `view` rect is maintained to be as large as possible, while always being
/// smaller than or equal to both view and screen.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ViewPort {
    screen: Rect,
    view: Rect,
    outer: Size,
}

impl Default for ViewPort {
    fn default() -> ViewPort {
        ViewPort {
            screen: Rect::default(),
            view: Rect::default(),
            outer: Size::default(),
        }
    }
}

impl ViewPort {
    /// Create a new View with the given outer and inner rectangles. The view
    /// rectangle must be fully contained within the outer rectangle.
    pub fn new(outer: Size, view: Rect, screen: Rect) -> Result<ViewPort> {
        if !outer.rect().contains_rect(&view) {
            Err(error::Error::Geometry("view not contained in outer".into()))
        } else {
            Ok(ViewPort {
                outer: outer,
                view: view,
                screen: screen,
            })
        }
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub fn scroll_to(&mut self, x: u16, y: u16) {
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        self.view = r.clamp_within(self.outer.rect()).unwrap();
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub fn scroll_by(&mut self, x: i16, y: i16) {
        self.view = self.view.shift_within(x, y, self.outer.rect())
    }

    /// Scroll the view up by the height of the view rectangle.
    pub fn page_up(&mut self) {
        self.scroll_by(0, -(self.view.h as i16));
    }

    /// Scroll the view down by the height of the view rectangle.
    pub fn page_down(&mut self) {
        self.scroll_by(0, self.view.h as i16);
    }

    /// Scroll the view up by one line.
    pub fn up(&mut self) {
        self.scroll_by(0, -1);
    }

    /// Scroll the view down by one line.
    pub fn down(&mut self) {
        self.scroll_by(0, 1);
    }

    /// Scroll the view left by one line.
    pub fn left(&mut self) {
        self.scroll_by(-1, 0);
    }

    /// Scroll the view right by one line.
    pub fn right(&mut self) {
        self.scroll_by(1, 0);
    }

    /// Return the inner view area.
    pub fn screen(&self) -> Rect {
        self.screen
    }

    /// Return the inner view area.
    pub fn view(&self) -> Rect {
        self.view
    }

    /// Return the enclosing area.
    pub fn outer(&self) -> Size {
        self.outer
    }

    /// Set the screen, view and outer rects all to the same size. This is
    /// useful for nodes that fill whatever space they're given.
    pub fn set_fill(&mut self, screen: Rect) {
        self.screen = screen;
        self.view = screen;
        self.outer = screen.into();
    }

    /// Set both the outer and screen rects at once. View position is
    /// maintained, but it's resized to be as large as possible.
    pub fn update(&mut self, size: Size, screen: Rect) {
        self.outer = size;
        self.screen = screen;

        // Now we maintain our view invariants. We know the size of the view is
        // the minimum in each dimension of the two enclosing rects.
        let w = size.w.min(screen.w);
        let h = size.h.min(screen.h);
        // Now we just clamp the rect into the view. We know the rect will fit,
        // so we unwrap.
        self.view = Rect {
            tl: self.view.tl,
            w,
            h,
        }
        .clamp_within(self.outer.rect())
        .unwrap();
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a vertical
    /// scroll bar for this viewport in the specified margin rect (usually a
    /// right or left vertical margin). Returns None if no scroll bar is needed.
    pub fn vactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        if self.view.h == self.outer.h {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .vextent()
                .split_active(self.view().vextent(), self.outer().rect().vextent())?;
            Ok(Some((
                margin.vextract(&pre)?,
                margin.vextract(&active)?,
                margin.vextract(&post)?,
            )))
        }
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a
    /// horizontal scroll bar for this viewport in the specified margin rect
    /// (usually a bottom horizontal margin). Returns None if no scroll bar is
    /// needed.
    pub fn hactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        if self.view.w == self.outer.w {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .hextent()
                .split_active(self.view().hextent(), self.outer().rect().hextent())?;
            Ok(Some((
                margin.hextract(&pre)?,
                margin.hextract(&active)?,
                margin.hextract(&post)?,
            )))
        }
    }

    /// Project a point in virtual space to the screen. If the point is not
    /// on-screen, return None.
    pub fn project_point(&self, p: Point) -> Option<Point> {
        if self.view.contains_point(p) {
            let rp = self.view.rebase_point(p).unwrap();
            // We know view is not larger than screen, so we can unwrap.
            Some(Point {
                x: self.screen.tl.x + rp.x,
                y: self.screen.tl.y + rp.y,
            })
        } else {
            None
        }
    }

    /// Project a rect in virtual space to the screen. If the virtual rect and
    /// the screen rect partially overlap, just the overlap is returned.
    pub fn project_rect(&self, r: Rect) -> Option<Rect> {
        if let Some(o) = self.view.intersect(&r) {
            let r = self.view.rebase_rect(&o).unwrap();
            Some(Rect {
                tl: self.screen.tl.scroll(r.tl.x as i16, r.tl.y as i16),
                w: r.w,
                h: r.h,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_project_rect() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(30, 30, 10, 10),
            Rect::new(50, 50, 10, 10),
        )?;

        assert!(v.project_rect(Rect::new(10, 10, 10, 10)).is_none());
        assert_eq!(
            v.project_rect(Rect::new(30, 30, 10, 10)),
            Some(Rect::new(50, 50, 10, 10))
        );
        assert_eq!(
            v.project_rect(Rect::new(20, 20, 15, 15)),
            Some(Rect::new(50, 50, 5, 5))
        );
        assert_eq!(
            v.project_rect(Rect::new(35, 35, 15, 15)),
            Some(Rect::new(55, 55, 5, 5))
        );

        Ok(())
    }

    #[test]
    fn view_project_point() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(30, 30, 10, 10),
            Rect::new(50, 50, 10, 10),
        )?;

        assert!(v.project_point(Point { x: 10, y: 10 }).is_none());
        assert_eq!(
            v.project_point(Point { x: 30, y: 30 }),
            Some(Point { x: 50, y: 50 }),
        );
        assert_eq!(
            v.project_point(Point { x: 35, y: 35 }),
            Some(Point { x: 55, y: 55 }),
        );
        assert_eq!(v.project_point(Point { x: 90, y: 90 }), None,);

        Ok(())
    }

    #[test]
    fn view_update() -> Result<()> {
        let mut v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(50, 50, 10, 10),
            Rect::new(50, 50, 10, 10),
        )?;

        v.update(Size::new(50, 50), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.update(Size::new(100, 100), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.update(Size::new(10, 10), Rect::new(0, 0, 10, 10));
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        v.update(Size::new(20, 20), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(0, 0, 20, 20));

        Ok(())
    }

    #[test]
    fn view_movement() -> Result<()> {
        let mut v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(0, 0, 10, 10),
            Rect::new(0, 0, 10, 10),
        )?;

        v.scroll_by(10, 10);
        assert_eq!(v.view, Rect::new(10, 10, 10, 10),);

        v.scroll_by(-20, -20);
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        v.page_down();
        assert_eq!(v.view, Rect::new(0, 10, 10, 10));

        v.page_up();
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        v.scroll_to(50, 50);
        assert_eq!(v.view, Rect::new(50, 50, 10, 10));

        v.scroll_to(150, 150);
        assert_eq!(v.view, Rect::new(90, 90, 10, 10));

        Ok(())
    }
}
