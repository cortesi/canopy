use crate::{
    Result, TermBuf, ViewPort, geom,
    style::Style,
    style::{StyleManager, StyleMap},
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{AttrSet, Color};
    use crate::{Expanse, TermBuf, ViewPort};

    fn setup_render_test(
        buf_size: Expanse,
        viewport_canvas: Expanse,
        viewport_view: geom::Rect,
    ) -> (TermBuf, StyleMap, StyleManager, ViewPort) {
        let default_style = Style {
            fg: Color::White,
            bg: Color::Black,
            attrs: AttrSet::default(),
        };
        let buf = TermBuf::new(buf_size, ' ', default_style.clone());

        let mut stylemap = StyleMap::default();
        // Add a default style to the map
        stylemap.add(
            "default",
            Some(Color::White),
            Some(Color::Black),
            Some(AttrSet::default()),
        );

        let style_manager = StyleManager::default();
        let viewport = ViewPort::new(viewport_canvas, viewport_view, geom::Point::zero()).unwrap();
        (buf, stylemap, style_manager, viewport)
    }

    #[test]
    fn test_fill_full_viewport() {
        let buf_size = Expanse::new(10, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 5));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Fill a rectangle in the middle of the buffer
        let rect = geom::Rect::new(2, 1, 4, 2);
        render.fill("default", rect, '#').unwrap();

        // Check that the rectangle was filled correctly
        buf.assert_buffer_matches(&[
            "          ",
            "  ####    ",
            "  ####    ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_fill_with_base_offset() {
        let buf_size = Expanse::new(10, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 5));

        let base = (1, 1).into();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Fill a rectangle at (0,0) which should appear at (1,1) due to base offset
        let rect = geom::Rect::new(0, 0, 3, 2);
        render.fill("default", rect, 'X').unwrap();

        // Check that the rectangle was filled at the offset position
        buf.assert_buffer_matches(&[
            "          ",
            " XXX      ",
            " XXX      ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_text_full_line() {
        let buf_size = Expanse::new(10, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 5));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Write text to a line
        let line = geom::Line {
            tl: geom::Point { x: 0, y: 1 },
            w: 10,
        };
        render.text("default", line, "Hello").unwrap();

        // Check that the text was written correctly
        buf.assert_buffer_matches(&[
            "          ",
            "Hello     ",
            "          ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_text_truncation() {
        let buf_size = Expanse::new(10, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 5));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Write text that's longer than the line
        let line = geom::Line {
            tl: geom::Point { x: 0, y: 0 },
            w: 5,
        };
        render.text("default", line, "Hello World").unwrap();

        // Check that only the first 5 characters were written
        buf.assert_buffer_matches(&[
            "Hello     ",
            "          ",
            "          ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_text_with_padding() {
        let buf_size = Expanse::new(10, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 5));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Write short text to a longer line
        let line = geom::Line {
            tl: geom::Point { x: 0, y: 2 },
            w: 8,
        };
        render.text("default", line, "Hi").unwrap();

        // Check that the text was written with padding
        buf.assert_buffer_matches(&[
            "          ",
            "          ",
            "Hi        ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_solid_frame() {
        let buf_size = Expanse::new(10, 10);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 10, 10));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Create a frame around a 6x6 area starting at (2,2)
        let frame = geom::Frame::new(geom::Rect::new(2, 2, 6, 6), 1);
        render.solid_frame("default", frame, '*').unwrap();

        // Check the frame is drawn correctly
        buf.assert_buffer_matches(&[
            "          ",
            "          ",
            "  ******  ",
            "  *    *  ",
            "  *    *  ",
            "  *    *  ",
            "  *    *  ",
            "  ******  ",
            "          ",
            "          ",
        ]);
    }

    #[test]
    fn test_solid_frame_single_width() {
        let buf_size = Expanse::new(5, 5);
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, buf_size, geom::Rect::new(0, 0, 5, 5));

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        // Create a minimal frame
        let frame = geom::Frame::new(geom::Rect::new(1, 1, 3, 3), 1);
        render.solid_frame("default", frame, '#').unwrap();

        // Check that frame is drawn correctly
        buf.assert_buffer_matches(&["     ", " ### ", " # # ", " ### ", "     "]);
    }
}
