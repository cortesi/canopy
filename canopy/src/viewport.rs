use crate::error;
use crate::geom::{Expanse, Line, Point, Rect};
use crate::Result;

/// A ViewPort manages the size of a node and its projection onto the screen.
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ViewPort {
    /// The location of the node in the parent's co-ordinate space. Must only be changed by the parent node.
    pub position: Point,
    /// The portion of this node that is displayed. A view within the size rectangle. Must only be changed by the node
    /// itself.
    pub view: Rect,
    /// The canvas on which children are positioned, and to which rendering occurs. Must only be changed by the node
    /// itself.
    pub canvas: Expanse,
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
        if !size.rect().contains_rect(&view) {
            Err(error::Error::Geometry(format!(
                "view {:?} not contained in size {:?}",
                view, size,
            )))
        } else {
            Ok(ViewPort {
                canvas: size,
                view,
                position: position.into(),
            })
        }
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub fn view_scroll_to(&mut self, x: u16, y: u16) {
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        self.view = r.clamp_within(self.canvas.rect()).unwrap();
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub fn view_scroll_by(&mut self, x: i16, y: i16) {
        self.view = self.view.shift_within(x, y, self.canvas.rect());
    }

    /// Scroll the view up by the height of the view rectangle.
    pub fn view_page_up(&mut self) {
        self.view_scroll_by(0, -(self.view.h as i16))
    }

    /// Scroll the view down by the height of the view rectangle.
    pub fn view_page_down(&mut self) {
        self.view_scroll_by(0, self.view.h as i16)
    }

    /// Scroll the view up by one line.
    pub fn view_up(&mut self) {
        self.view_scroll_by(0, -1)
    }

    /// Scroll the view down by one line.
    pub fn view_down(&mut self) {
        self.view_scroll_by(0, 1)
    }

    /// Scroll the view left by one line.
    pub fn view_left(&mut self) {
        self.view_scroll_by(-1, 0)
    }

    /// Scroll the view right by one line.
    pub fn view_right(&mut self) {
        self.view_scroll_by(1, 0)
    }

    /// Absolute rectangle for the screen region the node is being projected
    /// onto.
    pub fn screen_rect(&self) -> Rect {
        self.view.at(self.position)
    }

    /// Set the screen, view and outer rects all to the same size. This is
    /// useful for nodes that fill whatever space they're given.
    pub fn set_fill(&self, screen: Rect) -> Self {
        let mut vp = *self;
        vp.view = screen;
        vp.canvas = screen.into();
        vp
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

    /// Project a point in virtual space to the screen. If the point is not
    /// on-screen, return None.
    pub fn project_point(&self, p: impl Into<Point>) -> Option<Point> {
        let p = p.into();
        if self.view.contains_point(p) {
            let rp = self.view.rebase_point(p).unwrap();
            // We know view is not larger than screen, so we can unwrap.
            Some(Point {
                x: self.position.x + rp.x,
                y: self.position.y + rp.y,
            })
        } else {
            None
        }
    }

    /// Take a rectangle on the physical screen, and calculate the matching portion of the view rectangle.
    pub fn unproject(&self, r: Rect) -> Result<Rect> {
        self.screen_rect().rebase_rect(&r)
    }

    /// Project a rect in virtual space to the screen. If the virtual rect and
    /// the screen rect partially overlap, just the overlap is returned.
    pub fn project_rect(&self, r: Rect) -> Option<Rect> {
        if let Some(o) = self.view.intersect(&r) {
            let r = self.view.rebase_rect(&o).unwrap();
            Some(Rect {
                tl: self.position.scroll(r.tl.x as i16, r.tl.y as i16),
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
                    tl: self.position.scroll(rebase.tl.x as i16, rebase.tl.y as i16),
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
                canvas: child.expanse(),
                // The view is the intersection relative to the child's outer
                view: Rect::new(i.tl.x - child.tl.x, i.tl.y - child.tl.y, i.w, i.h),
                position: Point {
                    x: self.position.x + view_relative.tl.x,
                    y: self.position.y + view_relative.tl.y,
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
            canvas: v.expanse(),
            view: isect,
            position: Point {
                x: (isect.tl.x - self.view.tl.x) + self.position.x,
                y: (isect.tl.y - self.view.tl.y) + self.position.y,
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
    /// extent of this viewport. Returns a (left, right) tuple. Left is either
    /// empty or has the exact width specified.
    pub fn carve_hstart(&self, n: u16) -> (ViewPort, ViewPort) {
        let (a, b) = self.canvas.rect().carve_hstart(n);
        (self.view_to_vp(a), self.view_to_vp(b))
    }

    /// Carve a rectangle with a fixed width out of the end of the horizontal
    /// extent of this viewport. Returns a (left, right) tuple. Right is either
    /// empty or has the exact width specified.
    pub fn carve_hend(&self, n: u16) -> (ViewPort, ViewPort) {
        let (a, b) = self.canvas.rect().carve_hend(n);
        (self.view_to_vp(a), self.view_to_vp(b))
    }

    /// Carve a rectangle with a fixed width out of the start of the vertical
    /// extent of this viewport. Returns a (top, bottom) tuple. Top is either
    /// empty or has the exact width specified.
    pub fn carve_vstart(&self, n: u16) -> (ViewPort, ViewPort) {
        let (a, b) = self.canvas.rect().carve_vstart(n);
        (self.view_to_vp(a), self.view_to_vp(b))
    }

    /// Carve a rectangle with a fixed width out of the end of the vertical
    /// extent of this viewport. Returns a (top, bottom) tuple. Bottom is
    /// either empty or has the exact width specified.
    pub fn carve_vend(&self, n: u16) -> (ViewPort, ViewPort) {
        let (a, b) = self.canvas.rect().carve_vend(n);
        (self.view_to_vp(a), self.view_to_vp(b))
    }

    /// Splits the rectangle horizontally into n sections, as close to equally
    /// sized as possible.
    pub fn split_horizontal(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.canvas.rect().split_horizontal(n)?))
    }

    /// Splits the viewport vertically into n sections, as close to equally
    /// sized as possible.
    pub fn split_vertical(&self, n: u16) -> Result<Vec<ViewPort>> {
        Ok(self.views_to_vp(self.canvas.rect().split_vertical(n)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_map() -> Result<()> {
        let v = ViewPort::new(
            Expanse::new(100, 100),
            Rect::new(30, 30, 20, 20),
            (200, 200),
        )?;

        // No overlap with view
        assert!(v.map(Rect::new(10, 10, 2, 2),)?.is_none(),);

        assert_eq!(
            v.map(Rect::new(30, 30, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(40, 40, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (210, 210),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(25, 25, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(5, 5, 5, 5),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(35, 35, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 0, 10, 10),
                (205, 205),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(45, 45, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 0, 5, 5),
                (215, 215),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 21, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 9, 10, 1),
                (200, 200),
            )?)
        );

        assert_eq!(
            v.map(Rect::new(30, 49, 10, 10),)?,
            Some(ViewPort::new(
                Expanse::new(10, 10),
                Rect::new(0, 0, 10, 1),
                (200, 219),
            )?)
        );

        Ok(())
    }

    #[test]
    fn view_project_line() -> Result<()> {
        let v = ViewPort::new(Expanse::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

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
        let v = ViewPort::new(Expanse::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

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
        let v = ViewPort::new(Expanse::new(100, 100), Rect::new(30, 30, 10, 10), (50, 50))?;

        assert!(v.project_point((10, 10)).is_none());
        assert_eq!(v.project_point((30, 30)), Some(Point { x: 50, y: 50 }),);
        assert_eq!(v.project_point((35, 35)), Some(Point { x: 55, y: 55 }),);
        assert_eq!(v.project_point((90, 90)), None,);

        Ok(())
    }

    #[test]
    fn fit_size() -> Result<()> {
        let mut v = ViewPort::new(Expanse::new(100, 100), Rect::new(50, 50, 10, 10), (50, 50))?;

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
        fn tv<T>(vp: &ViewPort, f: &dyn Fn(&mut ViewPort) -> (), r: T)
        where
            T: Into<Rect>,
        {
            let mut v = vp.clone();
            f(&mut v);
            let r = r.into();
            assert_eq!(v.view, r);
        }

        let v = ViewPort::new(Expanse::new(100, 100), Rect::new(0, 0, 10, 10), (0, 0))?;

        tv(&v, &|v| v.view_scroll_by(10, 10), (10, 10, 10, 10));
        tv(&v, &|v| v.view_scroll_by(-20, -20), (0, 0, 10, 10));
        tv(&v, &|v| v.view_page_down(), (0, 10, 10, 10));
        tv(&v, &|v| v.view_page_up(), (0, 0, 10, 10));
        tv(&v, &|v| v.view_scroll_to(50, 50), (50, 50, 10, 10));
        tv(&v, &|v| v.view_scroll_to(150, 150), (90, 90, 10, 10));

        Ok(())
    }
}
