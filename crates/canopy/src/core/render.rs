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

/// A renderer that only renders to a specific rectangle within the target terminal buffer.
pub struct Render<'a> {
    /// The terminal buffer to render to.
    buf: TermBuf,
    /// The style manager used to apply styles.
    pub style: &'a mut StyleManager,
    /// The style map used to resolve style names to styles.
    stylemap: &'a StyleMap,
    /// The rectangle within the termbuf that we render to.
    rect: geom::Rect,
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
            buf,
            style,
            stylemap,
            rect,
        }
    }

    /// Fill a rectangle with a specified character. Writes out of bounds will be clipped.
    pub fn fill(&mut self, style: &str, r: geom::Rect, c: char) -> Result<()> {
        if let Some(intersection) = r.intersect(&self.rect) {
            let style = self.style.get(self.stylemap, style);
            // Adjust the intersection to be relative to our buffer's origin
            let adjusted = geom::Rect::new(
                intersection.tl.x - self.rect.tl.x,
                intersection.tl.y - self.rect.tl.y,
                intersection.w,
                intersection.h,
            );
            self.buf.fill(&style, adjusted, c);
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
        if let Some(intersection) = line_rect.intersect(&self.rect) {
            let style_res = self.style.get(self.stylemap, style);

            // Calculate how much of the text to skip and take
            let skip_amount = if l.tl.x < self.rect.tl.x {
                (self.rect.tl.x - l.tl.x) as usize
            } else {
                0
            };

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

            // Adjust the line position relative to our buffer
            let adjusted_line = geom::Line {
                tl: geom::Point {
                    x: intersection.tl.x - self.rect.tl.x,
                    y: intersection.tl.y - self.rect.tl.y,
                },
                w: intersection.w,
            };

            self.buf.text(&style_res, adjusted_line, out);

            // Pad with spaces if needed
            let out_width = out.chars().count();
            if out_width < adjusted_line.w as usize {
                let pad_rect = geom::Rect::new(
                    adjusted_line.tl.x + out_width as u32,
                    adjusted_line.tl.y,
                    adjusted_line.w - out_width as u32,
                    1,
                );
                self.buf.fill(&style_res, pad_rect, ' ');
            }
        }
        Ok(())
    }

    /// Get a reference to the internal buffer
    pub fn get_buffer(&self) -> &TermBuf {
        &self.buf
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
