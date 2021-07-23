use super::{Point, Rect, Size};
use crate::error;
use crate::Result;

/// ViewPort manages three rectangles in concert: `outer` is the total virtual
/// size of the node, `view` is some sub-rectangle of virtual that is painted to
/// `screen`, a rectangle on the physical screen.
///
/// ViewPort maintains a number of constraints:
///  - `view` is always contained within `outer`
///  - `view` and `screen` always have the same size
///  - `view`'s size only changes when `screen` is resized
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ViewPort {
    screen: Point,
    view: Rect,
    outer: Size,
}

impl Default for ViewPort {
    fn default() -> ViewPort {
        ViewPort {
            outer: Size::default(),
            view: Rect::default(),
            screen: Point::default(),
        }
    }
}

impl ViewPort {
    /// Create a new View with the given outer and inner rectangles. The view
    /// rectangle must be fully contained within the outer rectangle.
    pub fn new(outer: Size, view: Rect) -> Result<ViewPort> {
        if !outer.rect().contains_rect(&view) {
            Err(error::Error::Geometry("view not contained in outer".into()))
        } else {
            Ok(ViewPort {
                outer: outer,
                view: view,
                screen: Point::default(),
            })
        }
    }

    /// Scroll the view to the specified position. The view is clamped within
    /// the outer rectangle.
    pub fn scroll_to(&mut self, x: u16, y: u16) {
        let r = Rect::new(x, y, self.view.w, self.view.h);
        // We unwrap here, because this can only be an error if view is larger
        // than outer, which we ensure is not the case.
        self.view = r.clamp(self.outer.rect()).unwrap();
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
        self.view.at(&self.screen)
    }

    /// Return the inner view area.
    pub fn view(&self) -> Rect {
        self.view
    }

    /// Return the enclosing area.
    pub fn outer(&self) -> Size {
        self.outer
    }

    /// Set the screen rect and adjust the view and outer rects to be the same
    /// size. This is useful for nodes that fill whatever space they're given.
    pub fn set_fill(&mut self, screen: Rect) {
        self.screen = screen.tl;
        self.outer = screen.into();
        self.view = screen.into();
    }

    /// Set both the outer and screen rects at once.
    pub fn update(&mut self, size: Size, screen: Rect) -> Result<()> {
        self.outer = size;
        self.screen = screen.tl;
        self.view = screen.at(&self.view.tl).clamp(self.outer.rect())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_set_shell() -> Result<()> {
        let mut v = ViewPort::new(Size::new(100, 100), Rect::new(50, 50, 10, 10))?;

        let err = v.update(Size::new(10, 10), Rect::new(0, 0, 190, 190));
        assert!(err.is_err());

        v.update(Size::new(50, 50), Rect::new(0, 0, 20, 20))?;
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.update(Size::new(100, 100), Rect::new(0, 0, 20, 20))?;
        assert_eq!(v.view, Rect::new(30, 30, 20, 20));

        v.update(Size::new(10, 10), Rect::new(0, 0, 10, 10))?;
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        Ok(())
    }

    #[test]
    fn view_movement() -> Result<()> {
        let mut v = ViewPort::new(Size::new(100, 100), Rect::new(0, 0, 10, 10))?;

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
