use crate::error;
use crate::geom::{Line, Point, Rect, Size};
use crate::Result;

/// ViewPort manages three values in concert:
///  - `size` is the total virtual size of the node (equivalent to a Rect at 0,
///    0).
///  - `view` is some sub-rectangle of `size` that is being displayed.
///  - `screen`, is a rectangle on the physical screen that this node paints to,
///    equal in size to view.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ViewPort {
    screen: Point,
    view: Rect,
    size: Size,
}

impl Default for ViewPort {
    fn default() -> ViewPort {
        ViewPort {
            screen: Point::default(),
            view: Rect::default(),
            size: Size::default(),
        }
    }
}

impl ViewPort {
    /// Create a new View with the given outer and inner rectangles. The view
    /// rectangle must be fully contained within the outer rectangle.
    pub fn new(
        size: impl Into<Size>,
        view: impl Into<Rect>,
        screen: impl Into<Point>,
    ) -> Result<ViewPort> {
        let view = view.into();
        let size = size.into();
        if !size.rect().contains_rect(&view) {
            Err(error::Error::Geometry(format!(
                "view {:?} not contained in size {:?}",
                view, size,
            )))
        } else {
            Ok(ViewPort {
                size,
                view,
                screen: screen.into(),
            })
        }
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub fn scroll_to(&self, x: u16, y: u16) -> Self {
        let mut vp = *self;
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        vp.view = r.clamp_within(self.size.rect()).unwrap();
        vp
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub fn scroll_by(&self, x: i16, y: i16) -> Self {
        let mut vp = *self;
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
    pub fn screen_rect(&self) -> Rect {
        self.view.at(self.screen)
    }

    /// Return the view area.
    pub fn view_rect(&self) -> Rect {
        self.view
    }

    /// Return the enclosing area.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Set the screen, view and outer rects all to the same size. This is
    /// useful for nodes that fill whatever space they're given.
    pub fn set_fill(&self, screen: Rect) -> Self {
        let mut vp = *self;
        vp.screen = screen.tl;
        vp.view = screen;
        vp.size = screen.into();
        vp
    }

    /// Set both the outer and screen rects at once. View position is
    /// maintained, but it's resized to be as large as possible.
    pub fn update(&self, size: Size, screen: Rect) -> Self {
        let mut vp = *self;
        vp.size = size;
        vp.screen = screen.tl;

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
                .split_active(self.view_rect().vextent(), self.size().rect().vextent())?;
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
                .split_active(self.view_rect().hextent(), self.size().rect().hextent())?;
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
                x: self.screen.x + rp.x,
                y: self.screen.y + rp.y,
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
                tl: self.screen.scroll(r.tl.x as i16, r.tl.y as i16),
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
                    tl: self.screen.scroll(rebase.tl.x as i16, rebase.tl.y as i16),
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
                screen: Point {
                    x: self.screen.x + view_relative.tl.x,
                    y: self.screen.y + view_relative.tl.y,
                },
            }))
        } else {
            Ok(None)
        }
    }

    /// Turns a view rectangle into a sub-viewport. The outer size of the
    /// viewport remains the same.
    fn view_to_vp(&self, v: Rect) -> ViewPort {
        let isect = if let Some(r) = v.intersect(&self.view) {
            r
        } else {
            Rect::default()
        };
        ViewPort {
            size: v.size(),
            view: isect,
            screen: Point {
                x: (isect.tl.x - self.view.tl.x) + self.screen.x,
                y: (isect.tl.y - self.view.tl.y) + self.screen.y,
            },
        }
    }

    /// Turns a vector of view rectangles into sub-viewports.
    fn views_to_vp(&self, views: Vec<Rect>) -> Vec<ViewPort> {
        let mut ret = Vec::with_capacity(views.len());
        for i in views {
            ret.push(self.view_to_vp(i));
        }
        ret
    }

    /// Carve a rectangle with a fixed width out of the start of the horizontal
    /// extent of this viewport. Returns a [left, right] vector. Left is either
    /// empty or has the exact width specified.
    pub fn carve_hstart(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().carve_hstart(n).into()))
    }

    /// Carve a rectangle with a fixed width out of the end of the horizontal
    /// extent of this viewport. Returns a [left, right] vector. Right is either
    /// empty or has the exact width specified.
    pub fn carve_hend(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().carve_hend(n).into()))
    }

    /// Carve a rectangle with a fixed width out of the start of the vertical
    /// extent of this viewport. Returns a [top, bottom] vector. Top is either
    /// empty or has the exact width specified.
    pub fn carve_vstart(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().carve_vstart(n).into()))
    }

    /// Carve a rectangle with a fixed width out of the end of the vertical
    /// extent of this viewport. Returns a [top, bottom] vector. Bottom is
    /// either empty or has the exact width specified.
    pub fn carve_vend(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().carve_vend(n).into()))
    }

    /// Splits the rectangle horizontally into n sections, as close to equally
    /// sized as possible.
    pub fn split_horizontal(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().split_horizontal(n)?))
    }

    /// Splits the viewport vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.size().rect().split_vertical(n)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_map() -> Result<()> {
        let v = ViewPort::new(Size::new(100, 100), Rect::new(30, 30, 20, 20), (200, 200))?;

        // No overlap with view
        assert!(v.map(Rect::new(10, 10, 2, 2),)?.is_none(),);

        assert_eq!(
            v.map(Rect::new(30, 30, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(40, 40, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (210, 210),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(25, 25, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(5, 5, 5, 5),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(35, 35, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (205, 205),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(45, 45, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 5, 5),
                (215, 215),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 21, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 9, 10, 1),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 49, 10, 10),)?,
            Some(ViewPort::new(
                Size::new(10, 10),
                Rect::new(0, 0, 10, 1),
                (200, 219),
            )?)
        );

        Ok(())
    }

    #[test]
    fn view_project_line() -> Result<()> {
        let v = ViewPort::new(Size::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

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
        let v = ViewPort::new(Size::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

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
        let v = ViewPort::new(Size::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

        assert!(v.project_point((10, 10)).is_none());
        assert_eq!(v.project_point((30, 30)), Some(Point { x: 50, y: 50 }),);
        assert_eq!(v.project_point((35, 35)), Some(Point { x: 55, y: 55 }),);
        assert_eq!(v.project_point((90, 90)), None,);

        Ok(())
    }

    #[test]
    fn view_update() -> Result<()> {
        let v = ViewPort::new(Size::new(100, 100), Rect::new(50, 50, 10, 10), (50, 50))?;

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
        let v = ViewPort::new(Size::new(100, 100), Rect::new(0, 0, 10, 10), (0, 0))?;

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
