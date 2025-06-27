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
    buf: &'a mut TermBuf,
    pub style: &'a mut StyleManager,
    stylemap: &'a StyleMap,
    viewport: ViewPort,
    base: geom::Point,
}

impl<'a> Render<'a> {
    pub fn new(
        buf: &'a mut TermBuf,
        stylemap: &'a StyleMap,
        style: &'a mut StyleManager,
        viewport: ViewPort,
        base: geom::Point,
    ) -> Self {
        Render {
            buf,
            style,
            stylemap,
            viewport,
            base,
        }
    }

    /// Fill a rectangle with a specified character.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(dst) = self.viewport.project_rect(r) {
            let style = self.style.get(self.stylemap, style);
            let rect = dst.shift(self.base.x as i16, self.base.y as i16);
            self.buf.fill(style, rect, c);
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
            let style_res = self.style.get(self.stylemap, style);

            let out = txt
                .chars()
                .skip(offset as usize)
                .take(l.w as usize)
                .collect::<String>();

            let line = geom::Line {
                tl: self.base + dst.tl,
                w: dst.w,
            };
            self.buf.text(style_res.clone(), line, &out);
            if out.len() < dst.w as usize {
                let rect = geom::Rect::new(
                    self.base.x + dst.tl.x + out.len() as u16,
                    self.base.y + dst.tl.y,
                    dst.w - out.len() as u16,
                    1,
                );
                self.buf.fill(style_res, rect, ' ');
            }
        }
        Ok(())
    }
}
