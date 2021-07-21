use super::Rect;
use crate::error;
use crate::Result;

/// View manages two rectangles in concert - an outer rectangle and a view
/// rectangle that is free to move within the outer rectangle.
pub struct View {
    view: Rect,
    outer: Rect,
}

impl View {
    /// Create a new View with the given outer and inner rectangles. The view
    /// rectangle must be fully contained within the outer rectangle.
    pub fn new(outer: Rect, view: Rect) -> Result<View> {
        if !outer.contains_rect(&view) {
            Err(error::Error::Geometry("view not contained in outer".into()))
        } else {
            Ok(View {
                outer: outer,
                view: view,
            })
        }
    }
    pub fn scroll_to(&mut self, x: u16, y: u16) -> Result<()> {
        Ok(())
    }

    /// Scroll the view by the given offsets. The view rectangle is clamped
    /// within the outer rectangle.
    pub fn scroll_by(&mut self, x: i16, y: i16) {
        self.view = self.view.shift_within(x, y, self.outer)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_movement() -> Result<()> {
        let mut v = View::new(Rect::new(0, 0, 100, 100), Rect::new(0, 0, 10, 10))?;

        v.scroll_by(10, 10);
        assert_eq!(v.view, Rect::new(10, 10, 10, 10),);

        v.scroll_by(-20, -20);
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        v.page_down();
        assert_eq!(v.view, Rect::new(0, 10, 10, 10));

        v.page_up();
        assert_eq!(v.view, Rect::new(0, 0, 10, 10));

        Ok(())
    }
}
