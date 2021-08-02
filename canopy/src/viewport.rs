use crate::error;
use crate::geom::{Line, Point, Rect, Size};
use crate::Result;

/// ViewPort manages three rectangles in concert: `outer` is the total virtual
/// size of the node, `view` is some sub-rectangle of `outer`. The `screen`, is
/// a rectangle on the physical screen that this node paints to. It is larger
/// than or equal to view. If the screen is larger than the view, the view will
/// be positioned in the top-left corner of the screen.
///
/// The `view` rect is maintained to be as large as possible, while always being
/// smaller than or equal to both view and screen.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ViewPort {
    screen: Rect,
    view: Rect,
    size: Size,
}

impl Default for ViewPort {
    fn default() -> ViewPort {
        ViewPort {
            screen: Rect::default(),
            view: Rect::default(),
            size: Size::default(),
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
                size: outer,
                view: view,
                screen: screen,
            })
        }
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub fn scroll_to(&self, x: u16, y: u16) -> Self {
        let mut vp = self.clone();
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        vp.view = r.clamp_within(self.size.rect()).unwrap();
        vp
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub fn scroll_by(&self, x: i16, y: i16) -> Self {
        let mut vp = self.clone();
        vp.view = self.view.shift_within(x, y, self.size.rect());
        vp
    }

    /// Scroll the view up by the height of the view rectangle.
    pub fn page_up(&self) -> Self {
        self.scroll_by(0, -(self.view.h as i16))
    }

    /// Scroll the view down by the height of the view rectangle.
    pub fn page_down(&self) -> Self {
        self.scroll_by(0, self.view.h as i16)
    }

    /// Scroll the view up by one line.
    pub fn up(&self) -> Self {
        self.scroll_by(0, -1)
    }

    /// Scroll the view down by one line.
    pub fn down(&self) -> Self {
        self.scroll_by(0, 1)
    }

    /// Scroll the view left by one line.
    pub fn left(&self) -> Self {
        self.scroll_by(-1, 0)
    }

    /// Scroll the view right by one line.
    pub fn right(&self) -> Self {
        self.scroll_by(1, 0)
    }

    /// Return the screen region.
    pub fn screen(&self) -> Rect {
        self.screen
    }

    /// Return the view area.
    pub fn view(&self) -> Rect {
        self.view
    }

    /// Return the enclosing area.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Set the screen, view and outer rects all to the same size. This is
    /// useful for nodes that fill whatever space they're given.
    pub fn set_fill(&self, screen: Rect) -> Self {
        let mut vp = self.clone();
        vp.screen = screen;
        vp.view = screen;
        vp.size = screen.into();
        vp
    }

    /// Set both the outer and screen rects at once. View position is
    /// maintained, but it's resized to be as large as possible.
    pub fn update(&self, size: Size, screen: Rect) -> Self {
        let mut vp = self.clone();
        vp.size = size;
        vp.screen = screen;

        // Now we maintain our view invariants. We know the size of the view is
        // the minimum in each dimension of the two enclosing rects.
        let w = size.w.min(screen.w);
        let h = size.h.min(screen.h);
        // Now we just clamp the rect into the view. We know the rect will fit,
        // so we unwrap.
        vp.view = Rect {
            tl: self.view.tl,
            w,
            h,
        }
        .clamp_within(vp.size.rect())
        .unwrap();
        vp
    }

    /// Calculates the (pre, active, post) rectangles needed to draw a vertical
    /// scroll bar for this viewport in the specified margin rect (usually a
    /// right or left vertical margin). Returns None if no scroll bar is needed.
    pub fn vactive(&self, margin: Rect) -> Result<Option<(Rect, Rect, Rect)>> {
        if self.view.h == self.size.h {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .vextent()
                .split_active(self.view().vextent(), self.size().rect().vextent())?;
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
        if self.view.w == self.size.w {
            Ok(None)
        } else {
            let (pre, active, post) = margin
                .hextent()
                .split_active(self.view().hextent(), self.size().rect().hextent())?;
            Ok(Some((
                margin.hslice(&pre)?,
                margin.hslice(&active)?,
                margin.hslice(&post)?,
            )))
        }
    }

    /// Project a point in virtual space to the screen. If the point is not
    /// on-screen, return None.
    pub fn project_point(&self, p: impl Into<Point>) -> Option<Point> {
        let p = p.into();
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

    /// Project a line in virtual space to the screen. Returns an offset from
    /// the start of the input line, plus a Line that is the projected region.
    pub fn project_line(&self, l: Line) -> Option<(u16, Line)> {
        if let Some(o) = self.view.intersect(&l.rect()) {
            let rebase = self.view.rebase_rect(&o).unwrap();
            Some((
                o.tl.x - l.tl.x,
                Line {
                    tl: self
                        .screen
                        .tl
                        .scroll(rebase.tl.x as i16, rebase.tl.y as i16),
                    w: rebase.w,
                },
            ))
        } else {
            None
        }
    }

    /// Given a rectangle within our outer, calculate the intersection with our
    /// current view, and generate a ViewPort that would correctly display the
    /// child on our screen.
    pub fn map(&self, child: Rect) -> Result<Option<ViewPort>> {
        if let Some(i) = self.view.intersect(&child) {
            let view_relative = self.view.rebase_rect(&i).unwrap();
            Ok(Some(ViewPort {
                size: child.size(),
                // The view is the intersection relative to the child's outer
                view: Rect::new(i.tl.x - child.tl.x, i.tl.y - child.tl.y, i.w, i.h),
                screen: Rect::new(
                    self.screen.tl.x + view_relative.tl.x,
                    self.screen.tl.y + view_relative.tl.y,
                    i.w,
                    i.h,
                ),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_map() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(30, 30, 20, 20),
            Rect::new(200, 200, 20, 20),
        )?;

        // No overlap with view
        assert!(v.map(Rect::new(10, 10, 2, 2),)?.is_none(),);

        assert_eq!(
            v.map(Rect::new(30, 30, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                Rect::new(200, 200, 10, 10),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(40, 40, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                Rect::new(210, 210, 10, 10),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(25, 25, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(5, 5, 5, 5),
                Rect::new(200, 200, 5, 5),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(35, 35, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                Rect::new(205, 205, 10, 10),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(45, 45, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 5, 5),
                Rect::new(215, 215, 5, 5),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 21, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 9, 10, 1),
                Rect::new(200, 200, 10, 1),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 49, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 1),
                Rect::new(200, 219, 10, 1),
            )?)
        );

        Ok(())
    }

    #[test]
    fn view_project_line() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(30, 30, 10, 10),
            Rect::new(50, 50, 10, 10),
        )?;

        assert!(v.project_line(Line::new(10, 10, 10)).is_none());
        assert_eq!(
            v.project_line(Line::new(30, 30, 10)),
            Some((0, Line::new(50, 50, 10)))
        );
        assert_eq!(
            v.project_line(Line::new(20, 30, 15)),
            Some((10, Line::new(50, 50, 5)))
        );
        assert_eq!(
            v.project_line(Line::new(35, 30, 10)),
            Some((0, Line::new(55, 50, 5)))
        );

        Ok(())
    }

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

        assert!(v.project_point((10, 10)).is_none());
        assert_eq!(v.project_point((30, 30)), Some(Point { x: 50, y: 50 }),);
        assert_eq!(v.project_point((35, 35)), Some(Point { x: 55, y: 55 }),);
        assert_eq!(v.project_point((90, 90)), None,);

        Ok(())
    }

    #[test]
    fn view_update() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(50, 50, 10, 10),
            Rect::new(50, 50, 10, 10),
        )?;

        let v = v.update(Size::new(50, 50), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        let v = v.update(Size::new(100, 100), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        let v = v.update(Size::new(10, 10), Rect::new(0, 0, 10, 10));
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        let v = v.update(Size::new(20, 20), Rect::new(0, 0, 20, 20));
        assert_eq!(v.view, Rect::new(0, 0, 20, 20));

        Ok(())
    }

    #[test]
    fn view_movement() -> Result<()> {
        let v = ViewPort::new(
            Size::new(100, 100),
            Rect::new(0, 0, 10, 10),
            Rect::new(0, 0, 10, 10),
        )?;

        let v = v.scroll_by(10, 10);
        assert_eq!(v.view, Rect::new(10, 10, 10, 10),);

        let v = v.scroll_by(-20, -20);
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        let v = v.page_down();
        assert_eq!(v.view, Rect::new(0, 10, 10, 10));

        let v = v.page_up();
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        let v = v.scroll_to(50, 50);
        assert_eq!(v.view, Rect::new(50, 50, 10, 10));

        let v = v.scroll_to(150, 150);
        assert_eq!(v.view, Rect::new(90, 90, 10, 10));

        Ok(())
    }
}