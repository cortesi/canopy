use crate::{
    geom,
    style::Style,
    style::{StyleManager, StyleMap},
    Result, TermBuf, ViewPort,
};

/// The trait implemented by renderers.
pub trait RenderBackend {
    /// Apply a style to the following text output
    fn style(&mut self, style: Style) -> Result<()>;
    /// Output text to screen. This method is used for all text output.
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    /// Flush output to the terminal.
    fn flush(&mut self) -> Result<()>;
    /// Exit the process, relinquishing screen control.
    fn exit(&mut self, code: i32) -> !;
    fn reset(&mut self) -> Result<()>;
}

/// The interface used to render to the screen. It is only accessible in `Node::render`.
pub struct Render<'a> {
    buffer: &'a mut TermBuf,
    pub style: &'a mut StyleManager,
    stylemap: &'a StyleMap,
    viewport: ViewPort,
    base: geom::Point,
}


impl<'a> Render<'a> {
    pub fn new(
        buffer: &'a mut TermBuf,
        stylemap: &'a StyleMap,
        style: &'a mut StyleManager,
        viewport: ViewPort,
        base: geom::Point,
    ) -> Self {
        Render {
            buffer,
            style,
            stylemap,
            viewport,
            base,
        }
    }

    /// Fill a rectangle already projected onto the screen with a specified
    /// character. Assumes style has already been set.
    fn fill_screen(&mut self, dst: geom::Rect, c: char, style: Style) -> Result<()> {
        let rect = dst.shift(self.base.x as i16, self.base.y as i16);
        self.buffer.fill(style, rect, c);
        Ok(())
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(dst) = self.viewport.project_rect(r) {
            let st = self.style.get(self.stylemap, style);
            self.fill_screen(dst, c, st)?;
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
            let st = self.style.get(self.stylemap, style);
            let out = &txt
                .chars()
                .skip(offset as usize)
                .take(l.w as usize)
                .collect::<String>();
            let line = geom::Line::new(
                dst.tl.x + self.base.x,
                dst.tl.y + self.base.y,
                dst.w,
            );
            self.buffer.text(st.clone(), line, out);
            if out.len() < dst.w as usize {
                self.fill_screen(
                    geom::Rect::new(
                        dst.tl.x + out.len() as u16,
                        dst.tl.y,
                        dst.w - out.len() as u16,
                        1,
                    ),
                    ' ',
                    st,
                )?;
            }
        }
        Ok(())
    }
}
