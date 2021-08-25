use crate::{cursor, geom, style::Style, style::StyleManager, Result, ViewPort};

/// Backend is the interface implemented by renderers.
pub trait RenderBackend {
    fn style(&mut self, style: Style) -> Result<()>;
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    fn show_cursor(&mut self, c: cursor::Cursor) -> Result<()>;
    fn hide_cursor(&mut self) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn exit(&mut self, code: i32) -> !;
    fn reset(&mut self) -> Result<()>;
}

pub struct Render<'a> {
    pub backend: &'a mut dyn RenderBackend,
    pub style: StyleManager,
    pub viewport: ViewPort,
}

impl<'a> Render<'a> {
    pub fn new(backend: &mut dyn RenderBackend, style: StyleManager) -> Render {
        Render {
            backend,
            style,
            viewport: ViewPort::default(),
        }
    }

    /// Hide the cursor
    pub(crate) fn hide_cursor(&mut self) -> Result<()> {
        self.backend.hide_cursor()
    }

    /// Show the cursor with a specified style
    pub(crate) fn show_cursor(&mut self, style: &str, c: cursor::Cursor) -> Result<()> {
        if let Some(loc) = self.viewport.project_point(c.location) {
            let mut c = c;
            c.location = loc;
            self.backend.style(self.style.get(style))?;
            self.backend.show_cursor(c)?;
        }
        Ok(())
    }

    /// Fill a rectangle already projected onto the screen with a specified
    /// character. Assumes style has already been set.
    fn fill_screen(&mut self, dst: geom::Rect, c: char) -> Result<()> {
        let line = c.to_string().repeat(dst.w as usize);
        for n in 0..dst.h {
            self.backend.text((dst.tl.x, dst.tl.y + n).into(), &line)?;
        }
        Ok(())
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(dst) = self.viewport.project_rect(r) {
            self.backend.style(self.style.get(style))?;
            self.fill_screen(dst, c)?;
        }
        Ok(())
    }

    /// Draw a solid frame
    pub fn solid_frame(&mut self, style: &str, f: geom::Frame, c: char) -> Result<()> {
        self.fill(style, f.top, c)?;
        self.fill(style, f.left, c)?;
        self.fill(style, f.right, c)?;
        self.fill(style, f.bottom, c)?;
        self.fill(style, f.topleft, c)?;
        self.fill(style, f.topright, c)?;
        self.fill(style, f.bottomleft, c)?;
        self.fill(style, f.bottomright, c)?;
        Ok(())
    }

    /// Print text in the specified line. If the text is wider than the
    /// rectangle, it will be truncated; if it is shorter, it will be padded.
    pub fn text(&mut self, style: &str, l: geom::Line, txt: &str) -> Result<()> {
        if let Some((offset, dst)) = self.viewport.project_line(l) {
            self.backend.style(self.style.get(style))?;

            let out = &txt
                .chars()
                .skip(offset as usize)
                .take(l.w as usize)
                .collect::<String>();

            self.backend.text(dst.tl, out)?;
            if out.len() < dst.w as usize {
                self.fill_screen(
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

    pub(crate) fn push(&mut self) {
        self.style.push();
    }

    pub(crate) fn pop(&mut self) {
        self.style.pop();
    }

    pub(crate) fn reset(&mut self) -> Result<()> {
        self.backend.reset()?;
        self.style.reset();
        Ok(())
    }

    pub(crate) fn flush(&mut self) -> Result<()> {
        self.backend.flush()
    }
}
