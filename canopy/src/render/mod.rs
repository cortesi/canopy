use crate::{cursor, geom, style::Color, style::Style, Result, ViewPort};

pub mod term;
pub mod test;

pub trait Backend {
    fn fg(&mut self, c: Color) -> Result<()>;
    fn bg(&mut self, c: Color) -> Result<()>;
    fn fill(&mut self, r: geom::Rect, c: char) -> Result<()>;
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()>;
    fn hide_cursor(&mut self) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn exit(&mut self, code: i32) -> !;
    fn reset(&mut self) -> Result<()>;
}

pub struct Render<'a> {
    pub backend: &'a mut dyn Backend,
    pub style: Style,
    pub viewport: ViewPort,
}

impl<'a> Render<'a> {
    pub fn new(backend: &mut dyn Backend, style: Style) -> Render {
        Render {
            backend,
            style,
            viewport: ViewPort::default(),
        }
    }

    /// Fill a rectangle with a specified character.
    pub fn hide_cursor(&mut self) -> Result<()> {
        self.backend.hide_cursor()
    }

    /// Fill a rectangle with a specified character.
    pub fn show_cursor(&mut self, color: &str, c: cursor::Cursor) -> Result<()> {
        if let Some(loc) = self.viewport.project_point(c.location) {
            let mut c = c;
            c.location = loc;
            self.backend.fg(self.style.fg(color))?;
            self.backend.bg(self.style.bg(color))?;
            self.backend.show_cursor(c)?;
        }
        Ok(())
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, color: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(dst) = self.viewport.project_rect(r) {
            self.backend.fg(self.style.fg(color))?;
            self.backend.bg(self.style.bg(color))?;
            self.backend.fill(dst, c)?;
        }
        Ok(())
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

    /// Print text in the specified line. If the text is wider than the
    /// rectangle, it will be truncated; if it is shorter, it will be padded.
    pub fn text(&mut self, color: &str, l: geom::Line, txt: &str) -> Result<()> {
        if let Some((offset, dst)) = self.viewport.project_line(l) {
            self.backend.fg(self.style.fg(color))?;
            self.backend.bg(self.style.bg(color))?;

            let out = &txt
                .chars()
                .skip(offset as usize)
                .take(l.w as usize)
                .collect::<String>();

            self.backend.text(dst.tl, out)?;
            if out.len() < dst.w as usize {
                self.backend.fill(
                    geom::Rect::new(
                        dst.tl.x + out.len() as u16,
                        dst.tl.y,
                        dst.w - out.len() as u16,
                        1,
                    ),
                    ' ',
                )?;
            }
        }
        Ok(())
    }

    pub fn push(&mut self) {
        self.style.push();
    }

    pub fn pop(&mut self) {
        self.style.pop();
    }

    pub fn reset(&mut self) -> Result<()> {
        self.backend.reset()?;
        self.style.reset();
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.backend.flush()
    }

    pub fn exit(&mut self, code: i32) -> ! {
        self.backend.exit(code)
    }
}
