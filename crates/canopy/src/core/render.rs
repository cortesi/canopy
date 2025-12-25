use super::termbuf::TermBuf;
use crate::{
    error::Result,
    geom,
    style::{AttrSet, Color, Style, StyleManager, StyleMap},
};

/// The trait implemented by renderers.
pub trait RenderBackend {
    /// Apply a style to the following text output
    fn style(&mut self, style: &Style) -> Result<()>;
    /// Output text to screen. This method is used for all text output.
    fn text(&mut self, loc: geom::Point, txt: &str) -> Result<()>;
    /// Flush output to the terminal.
    fn flush(&mut self) -> Result<()>;
    /// Exit the process, relinquishing screen control.
    fn exit(&mut self, code: i32) -> !;
    /// Reset the backend to a clean state.
    fn reset(&mut self) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Signed translation offset in cell coordinates.
struct Offset {
    /// Horizontal offset.
    x: i32,
    /// Vertical offset.
    y: i32,
}

impl Offset {
    /// Compute the translation from a source point to a destination point.
    fn between(dest: geom::Point, src: geom::Point) -> Self {
        Self {
            x: dest.x as i32 - src.x as i32,
            y: dest.y as i32 - src.y as i32,
        }
    }
}

/// Buffer target for rendering operations.
enum RenderTarget<'a> {
    /// Owned offscreen buffer.
    Owned(TermBuf),
    /// Shared destination buffer.
    Shared(&'a mut TermBuf),
}

/// A renderer that only renders to a specific rectangle within the target terminal buffer.
pub struct Render<'a> {
    /// The terminal buffer to render to.
    target: RenderTarget<'a>,
    /// The style manager used to apply styles.
    style: &'a mut StyleManager,
    /// The style map used to resolve style names to styles.
    stylemap: &'a StyleMap,
    /// The rectangle in canvas coordinates that is visible for rendering.
    clip: geom::Rect,
    /// Translation offset from canvas coordinates to buffer coordinates.
    origin: Offset,
}

impl<'a> Render<'a> {
    /// Construct a renderer for the given rectangle.
    pub fn new(stylemap: &'a StyleMap, style: &'a mut StyleManager, rect: geom::Rect) -> Self {
        let buf = TermBuf::new(
            (rect.w, rect.h),
            '\0',
            Style {
                fg: Color::White,
                bg: Color::Black,
                attrs: AttrSet::default(),
            },
        );
        Render {
            target: RenderTarget::Owned(buf),
            style,
            stylemap,
            clip: rect,
            origin: Offset::between(geom::Point::zero(), rect.tl),
        }
    }

    /// Construct a renderer that writes directly into a shared buffer.
    pub(crate) fn new_shared(
        stylemap: &'a StyleMap,
        style: &'a mut StyleManager,
        buf: &'a mut TermBuf,
        clip: geom::Rect,
        screen_origin: geom::Point,
    ) -> Self {
        Render {
            target: RenderTarget::Shared(buf),
            style,
            stylemap,
            clip,
            origin: Offset::between(screen_origin, clip.tl),
        }
    }

    /// Push a style layer.
    pub fn push_layer(&mut self, name: &str) {
        self.style.push_layer(name);
    }

    /// Fill a rectangle with a specified character. Writes out of bounds will be clipped.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(intersection) = r.intersect(&self.clip) {
            let style = self.style.get(self.stylemap, style);
            let adjusted = self.translate_rect(intersection);
            self.buffer_mut().fill(&style, adjusted, c);
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
        let line_rect = geom::Rect::new(l.tl.x, l.tl.y, l.w, 1);
        if let Some(intersection) = line_rect.intersect(&self.clip) {
            let style_res = self.style.get(self.stylemap, style);

            // Calculate how much of the text to skip and take
            let skip_amount = intersection.tl.x.saturating_sub(l.tl.x) as usize;
            let take_amount = intersection.w as usize;

            let start_byte = txt
                .char_indices()
                .nth(skip_amount)
                .map(|(i, _)| i)
                .unwrap_or(txt.len());
            let end_byte = txt
                .char_indices()
                .nth(skip_amount + take_amount)
                .map(|(i, _)| i)
                .unwrap_or(txt.len());
            let out = &txt[start_byte..end_byte];

            let adjusted_line = geom::Line {
                tl: self.translate_point(intersection.tl),
                w: intersection.w,
            };

            self.buffer_mut().text(&style_res, adjusted_line, out);

            // Pad with spaces if needed
            let out_width = out.chars().count();
            if out_width < adjusted_line.w as usize {
                let pad_rect = geom::Rect::new(
                    adjusted_line.tl.x + out_width as u32,
                    adjusted_line.tl.y,
                    adjusted_line.w - out_width as u32,
                    1,
                );
                self.buffer_mut().fill(&style_res, pad_rect, ' ');
            }
        }
        Ok(())
    }

    /// Get a reference to the internal buffer
    pub fn get_buffer(&self) -> &TermBuf {
        self.buffer()
    }

    /// Access the underlying buffer.
    fn buffer(&self) -> &TermBuf {
        match &self.target {
            RenderTarget::Owned(buf) => buf,
            RenderTarget::Shared(buf) => buf,
        }
    }

    /// Access the underlying buffer mutably.
    fn buffer_mut(&mut self) -> &mut TermBuf {
        match &mut self.target {
            RenderTarget::Owned(buf) => buf,
            RenderTarget::Shared(buf) => buf,
        }
    }

    /// Translate a point from canvas coordinates to buffer coordinates.
    fn translate_point(&self, p: geom::Point) -> geom::Point {
        let x = p.x as i32 + self.origin.x;
        let y = p.y as i32 + self.origin.y;
        debug_assert!(
            x >= 0 && y >= 0,
            "translated point out of bounds: {:?} + {:?}",
            p,
            self.origin
        );
        geom::Point {
            x: x.max(0) as u32,
            y: y.max(0) as u32,
        }
    }

    /// Translate a rectangle from canvas coordinates to buffer coordinates.
    fn translate_rect(&self, rect: geom::Rect) -> geom::Rect {
        geom::Rect {
            tl: self.translate_point(rect.tl),
            w: rect.w,
            h: rect.h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buf,
        style::{StyleManager, StyleMap},
        testing::buf::BufTest,
    };

    fn assert_buffer_matches(render: &Render, expected: &[&str]) {
        BufTest::new(render.get_buffer()).assert_matches(expected);
    }

    #[test]
    fn test_part_render_fill_within_bounds() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Fill entirely within the render rectangle
        let result = part_render.fill("default", geom::Rect::new(6, 6, 3, 3), '#');
        assert!(result.is_ok());

        // Check that the buffer was filled correctly (adjusted to buffer coordinates)
        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "X###XXXXXX"
                "X###XXXXXX"
                "X###XXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_part_render_fill_partial_overlap() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Fill that partially overlaps the render rectangle
        let result = part_render.fill("default", geom::Rect::new(3, 3, 5, 5), '#');
        assert!(result.is_ok());

        // Should only show the part that overlaps with render rect
        assert_buffer_matches(
            &part_render,
            buf!(
                "###XXXXXXX"
                "###XXXXXXX"
                "###XXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );

        // Fill that starts inside but extends beyond render rect
        let result = part_render.fill("default", geom::Rect::new(10, 10, 8, 8), 'Y');
        assert!(result.is_ok());

        assert_buffer_matches(
            &part_render,
            buf!(
                "###XXXXXXX"
                "###XXXXXXX"
                "###XXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXYYYYY"
                "XXXXXYYYYY"
                "XXXXXYYYYY"
                "XXXXXYYYYY"
                "XXXXXYYYYY"
            ),
        );
    }

    #[test]
    fn test_part_render_fill_outside_render_rect() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Fill completely outside the render rectangle but within canvas
        let result = part_render.fill("default", geom::Rect::new(0, 0, 3, 3), '#');
        assert!(result.is_ok()); // Should succeed but not affect the buffer

        // Another test outside render rect
        let result = part_render.fill("default", geom::Rect::new(16, 16, 3, 3), 'Y');
        assert!(result.is_ok());

        // Buffer should remain unchanged (all NULL)
        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_shared_render_clips_to_canvas_rect() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let default_style = style_manager.get(&stylemap, "");
        let mut target = TermBuf::empty_with_style(geom::Expanse::new(6, 4), default_style);

        let clip = geom::Rect::new(2, 1, 2, 2);
        let screen_origin = geom::Point { x: 3, y: 0 };
        {
            let mut render = Render::new_shared(
                &stylemap,
                &mut style_manager,
                &mut target,
                clip,
                screen_origin,
            );
            render
                .fill("default", geom::Rect::new(0, 0, 6, 4), '#')
                .unwrap();
            render
                .text("default", geom::Line::new(1, 2, 4), "abcd")
                .unwrap();
        }

        BufTest::new(&target).assert_matches(buf!(
            "XXX##X"
            "XXXbcX"
            "XXXXXX"
            "XXXXXX"
        ));
    }

    #[test]
    fn test_part_render_fill_outside_canvas() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Fill that extends beyond canvas bounds
        part_render
            .fill("default", geom::Rect::new(15, 15, 10, 10), '#')
            .unwrap();

        // Fill completely outside canvas
        part_render
            .fill("default", geom::Rect::new(25, 25, 5, 5), 'Y')
            .unwrap();

        // Fill that starts at edge and extends beyond
        part_render
            .fill("default", geom::Rect::new(19, 19, 2, 2), 'Z')
            .unwrap();

        // Buffer should remain unchanged
        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_part_render_text_within_bounds() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Text entirely within render rectangle
        let result = part_render.text(
            "default",
            geom::Line {
                tl: geom::Point { x: 6, y: 6 },
                w: 5,
            },
            "Hello",
        );
        assert!(result.is_ok());

        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "XHelloXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );

        // Text that exactly fits
        let result = part_render.text(
            "default",
            geom::Line {
                tl: geom::Point { x: 5, y: 5 },
                w: 10,
            },
            "1234567890",
        );
        assert!(result.is_ok());

        assert_buffer_matches(
            &part_render,
            buf!(
                "1234567890"
                "XHelloXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_part_render_text_partial_overlap() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Text that starts before render rect
        let result = part_render.text(
            "default",
            geom::Line {
                tl: geom::Point { x: 3, y: 6 },
                w: 10,
            },
            "1234567890",
        );
        assert!(result.is_ok());

        // Should show chars starting from index 2 (skip first 2 chars)
        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "34567890XX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );

        // Text that extends beyond render rect
        let result = part_render.text(
            "default",
            geom::Line {
                tl: geom::Point { x: 10, y: 10 },
                w: 8,
            },
            "LongText",
        );
        assert!(result.is_ok());

        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "34567890XX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXLongT"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_part_render_text_outside_canvas() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Text that extends beyond canvas
        part_render
            .text(
                "default",
                geom::Line {
                    tl: geom::Point { x: 15, y: 15 },
                    w: 10,
                },
                "Text",
            )
            .unwrap();

        // Text completely outside canvas
        part_render
            .text(
                "default",
                geom::Line {
                    tl: geom::Point { x: 25, y: 25 },
                    w: 5,
                },
                "Text",
            )
            .unwrap();

        // Buffer should remain unchanged
        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        );
    }

    #[test]
    fn test_part_render_solid_frame() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();
        let render_rect = geom::Rect::new(5, 5, 10, 10);

        let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

        // Frame within bounds
        let frame = geom::Frame::new(geom::Rect::new(6, 6, 8, 8), 1);
        let result = part_render.solid_frame("default", frame, '#');
        assert!(result.is_ok());

        assert_buffer_matches(
            &part_render,
            buf!(
                "XXXXXXXXXX"
                "X########X"
                "X#XXXXXX#X"
                "X#XXXXXX#X"
                "X#XXXXXX#X"
                "X#XXXXXX#X"
                "X#XXXXXX#X"
                "X#XXXXXX#X"
                "X########X"
                "XXXXXXXXXX"
            ),
        );

        // Frame that extends outside canvas should fail
        let frame = geom::Frame::new(geom::Rect::new(15, 15, 10, 10), 1);
        part_render.solid_frame("default", frame, '#').unwrap();
    }

    #[test]
    fn test_part_render_multiple_rectangles() {
        let stylemap = StyleMap::new();
        let mut style_manager = StyleManager::new();

        // Test with render rect at different positions
        let positions = vec![
            (geom::Rect::new(0, 0, 10, 10), "top-left"), // Top-left corner
            (geom::Rect::new(10, 10, 10, 10), "center"), // Center
            (geom::Rect::new(20, 20, 10, 10), "bottom-right"), // Bottom-right corner
        ];

        for (render_rect, position) in positions {
            let mut part_render = Render::new(&stylemap, &mut style_manager, render_rect);

            // Fill within the specific render rect
            let fill_rect = geom::Rect::new(render_rect.tl.x + 1, render_rect.tl.y + 1, 5, 5);
            let result = part_render.fill("default", fill_rect, '#');
            assert!(result.is_ok());

            let expected = buf!(
                "XXXXXXXXXX"
                "X#####XXXX"
                "X#####XXXX"
                "X#####XXXX"
                "X#####XXXX"
                "X#####XXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            );

            match position {
                "top-left" | "center" | "bottom-right" => {
                    assert_buffer_matches(&part_render, expected);
                }
                _ => panic!("Unknown position: {position}"),
            }

            // Fill outside canvas should be ignored
            part_render
                .fill("default", geom::Rect::new(40, 40, 5, 5), 'Y')
                .unwrap();

            assert_buffer_matches(&part_render, expected);
        }
    }
}
