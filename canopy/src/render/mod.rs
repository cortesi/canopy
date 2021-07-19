use crate::{cursor, geom, style::Color, style::Style, Result};

pub mod term;
pub mod tst;

pub trait Backend {
    fn fg(&mut self, c: Color) -> Result<()>;
    fn bg(&mut self, c: Color) -> Result<()>;
    fn fill(&mut self, r: geom::Rect, c: char) -> Result<()>;
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()>;
    fn hide_cursor(&mut self) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn exit(&mut self, code: i32) -> !;
}

pub struct Render<'a> {
    backend: &'a mut dyn Backend,
    pub style: Style,
}

impl<'a> Render<'a> {
    pub fn new(backend: &mut dyn Backend, style: Style) -> Render {
        Render { backend, style }
    }

    /// Fill a rectangle with a specified character.
    pub fn hide_cursor(&mut self) -> Result<()> {
        self.backend.hide_cursor()
    }

    /// Fill a rectangle with a specified character.
    pub fn show_cursor(&mut self, color: &str, c: cursor::Cursor) -> Result<()> {
        self.backend.fg(self.style.fg(color))?;
        self.backend.bg(self.style.bg(color))?;
        self.backend.show_cursor(c)?;
        Ok(())
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, color: &str, r: geom::Rect, c: char) -> Result<()> {
        self.backend.fg(self.style.fg(color))?;
        self.backend.bg(self.style.bg(color))?;
        self.backend.fill(r, c)
    }

    /// Draw a solid frame
    pub fn solid_frame(&mut self, color: &str, f: geom::Frame, c: char) -> Result<()> {
        self.fill(color, f.top, c)?;
        self.fill(color, f.left, c)?;
        self.fill(color, f.right, c)?;
        self.fill(color, f.bottom, c)?;
        self.fill(color, f.topleft, c)?;
        self.fill(color, f.topright, c)?;
        self.fill(color, f.bottomleft, c)?;
        self.fill(color, f.bottomright, c)?;
        Ok(())
    }

    /// Print text in the first line of the specified rectangle. If the text is
    /// wider than the rectangle, it will be truncated; if it is shorter, it
    /// will be padded.
    pub fn text(&mut self, color: &str, r: geom::Rect, txt: &str) -> Result<()> {
        self.backend.fg(self.style.fg(color))?;
        self.backend.bg(self.style.bg(color))?;

        let out = &txt.chars().take(r.w as usize).collect::<String>();
        self.backend.text(r.tl, out)?;
        if out.len() < r.w as usize {
            self.backend.fill(
                geom::Rect {
                    tl: geom::Point {
                        x: r.tl.x + out.len() as u16,
                        y: r.tl.y,
                    },
                    w: r.w - out.len() as u16,
                    h: r.h,
                },
                ' ',
            )?;
        }
        Ok(())
    }

    pub fn push(&mut self) {
        self.style.push();
    }
    pub fn pop(&mut self) {
        self.style.pop();
    }
    pub fn reset(&mut self) {
        self.style.reset();
    }
    pub fn flush(&mut self) -> Result<()> {
        self.backend.flush()
    }
}
