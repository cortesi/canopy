//! Tests for the render surface.

use super::*;
use crate::{
    buf,
    style::{AttrSet, Color, StyleManager, StyleMap},
    testing::buf::BufTest,
};

/// Offscreen render target sized to one clip rectangle.
struct TestTarget {
    /// Style rules resolved during rendering.
    stylemap: StyleMap,
    /// Layer stack shared across operations.
    style: StyleManager,
    /// Destination buffer.
    buf: TermBuf,
    /// Visible rectangle in canvas coordinates.
    clip: geom::Rect,
}

impl TestTarget {
    fn new(clip: geom::Rect) -> Self {
        let buf = TermBuf::new(
            (clip.w, clip.h),
            '\0',
            ResolvedStyle::new(Color::White, Color::Black, AttrSet::default()),
        )
        .expect("test render target should allocate");
        Self {
            stylemap: StyleMap::new(),
            style: StyleManager::new(),
            buf,
            clip,
        }
    }

    /// Run one operation against a renderer bound to this target.
    fn render<R>(&mut self, f: impl FnOnce(&mut Render<'_>) -> R) -> R {
        let mut render = Render::new(
            &self.stylemap,
            &mut self.style,
            &mut self.buf,
            self.clip,
            geom::Point::zero(),
        );
        f(&mut render)
    }

    /// Resolve the default style through this target's style manager.
    fn default_style(&self) -> ResolvedStyle {
        self.style
            .get(&self.stylemap, "")
            .resolve_solid()
            .expect("default style resolves to solid colors")
    }

    fn assert_matches(&self, expected: &[&str]) {
        BufTest::new(&self.buf).assert_matches(expected);
    }
}

#[test]
fn test_part_render_fill_within_bounds() {
    let mut target = TestTarget::new(geom::Rect::new(5, 5, 10, 10));

    // Fill entirely within the render rectangle
    target
        .render(|r| r.fill("default", geom::Rect::new(6, 6, 3, 3), '#'))
        .unwrap();

    // Check that the buffer was filled correctly (adjusted to buffer
    // coordinates)
    target.assert_matches(buf!(
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
    ));
}

#[test]
fn test_part_render_fill_partial_overlap() {
    let mut target = TestTarget::new(geom::Rect::new(5, 5, 10, 10));

    // Fill that partially overlaps the render rectangle
    target
        .render(|r| r.fill("default", geom::Rect::new(3, 3, 5, 5), '#'))
        .unwrap();

    // Should only show the part that overlaps with render rect
    target.assert_matches(buf!(
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
    ));

    // Fill that starts inside but extends beyond render rect
    target
        .render(|r| r.fill("default", geom::Rect::new(10, 10, 8, 8), 'Y'))
        .unwrap();

    target.assert_matches(buf!(
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
    ));
}

#[test]
fn test_part_render_fill_outside_render_rect() {
    let mut target = TestTarget::new(geom::Rect::new(5, 5, 10, 10));

    // Fill completely outside the render rectangle but within canvas
    target
        .render(|r| r.fill("default", geom::Rect::new(0, 0, 3, 3), '#'))
        .unwrap(); // Should succeed but not affect the buffer

    // Another test outside render rect
    target
        .render(|r| r.fill("default", geom::Rect::new(16, 16, 3, 3), 'Y'))
        .unwrap();

    // Buffer should remain unchanged (all NULL)
    target.assert_matches(buf!(
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
    ));
}

#[test]
fn test_shared_render_clips_to_canvas_rect() {
    let stylemap = StyleMap::new();
    let mut style_manager = StyleManager::new();
    let default_style = style_manager
        .get(&stylemap, "")
        .resolve_solid()
        .expect("default style resolves to solid colors");
    let mut target = TermBuf::new(geom::Size::new(6, 4), '\0', default_style)
        .expect("test render target should allocate");

    let clip = geom::Rect::new(2, 1, 2, 2);
    let screen_origin = geom::Point { x: 3, y: 0 };
    {
        let mut render = Render::new(
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
fn put_grapheme_clips_wide_glyphs_atomically() {
    let mut target = TestTarget::new(geom::Rect::new(0, 0, 2, 1));
    let style = target.default_style();

    target
        .render(|r| r.put_grapheme(style, geom::Point { x: 1, y: 0 }, "界"))
        .unwrap();
    target.assert_matches(buf!("XX"));

    target
        .render(|r| r.put_grapheme(style, geom::Point { x: 0, y: 0 }, "界"))
        .unwrap();
    target.assert_matches(buf!("界X"));
}

#[test]
fn fill_draws_the_parts_of_a_frame() {
    let mut target = TestTarget::new(geom::Rect::new(5, 5, 10, 10));

    let frame = geom::FrameRects::new(geom::Rect::new(6, 6, 8, 8), 1);
    for part in [frame.top, frame.left, frame.right, frame.bottom] {
        target.render(|r| r.fill("default", part, '#')).unwrap();
    }

    target.assert_matches(buf!(
        "XXXXXXXXXX"
        "XX######XX"
        "X#XXXXXX#X"
        "X#XXXXXX#X"
        "X#XXXXXX#X"
        "X#XXXXXX#X"
        "X#XXXXXX#X"
        "X#XXXXXX#X"
        "XX######XX"
        "XXXXXXXXXX"
    ));
}

/// One text-rendering case: a clip rectangle, a line, and the expected buffer.
struct TextCase {
    /// Case name reported on failure.
    name: &'static str,
    /// Visible rectangle in canvas coordinates.
    clip: geom::Rect,
    /// Line the text is drawn on, in canvas coordinates.
    line: geom::Line,
    /// Text drawn on the line.
    text: &'static str,
    /// Expected buffer contents.
    expected: &'static [&'static str],
}

impl TextCase {
    fn run(&self) {
        let mut target = TestTarget::new(self.clip);
        target
            .render(|r| r.text("default", self.line, self.text))
            .unwrap();
        BufTest::new(&target.buf).assert_matches_with_context(self.expected, Some(self.name));
    }
}

/// Build a line at `(x, y)` with the given width.
fn line(x: u32, y: u32, w: u32) -> geom::Line {
    geom::Line {
        tl: geom::Point { x, y },
        w,
    }
}

#[test]
fn text_truncates_pads_and_clips() {
    let clip = geom::Rect::new(0, 0, 5, 5);
    let cases = [
        TextCase {
            name: "full line",
            clip,
            line: line(0, 1, 5),
            text: "Hello",
            expected: buf!("XXXXX" "Hello" "XXXXX" "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "overflow",
            clip,
            line: line(0, 0, 5),
            text: "Hello World",
            expected: buf!("Hello" "XXXXX" "XXXXX" "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "truncation",
            clip,
            line: line(0, 0, 2),
            text: "Hello World",
            expected: buf!("HeXXX" "XXXXX" "XXXXX" "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "zero width",
            clip,
            line: line(0, 0, 0),
            text: "Hello World",
            expected: buf!("XXXXX" "XXXXX" "XXXXX" "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "padding",
            clip,
            line: line(0, 2, 5),
            text: "Hi",
            expected: buf!("XXXXX" "XXXXX" "Hi   " "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "below the clip",
            clip,
            line: line(0, 5, 5),
            text: "Hi",
            expected: buf!("XXXXX" "XXXXX" "XXXXX" "XXXXX" "XXXXX"),
        },
        TextCase {
            name: "right of the clip",
            clip,
            line: line(10, 0, 5),
            text: "Hi",
            expected: buf!("XXXXX" "XXXXX" "XXXXX" "XXXXX" "XXXXX"),
        },
    ];
    for case in &cases {
        case.run();
    }
}

#[test]
fn text_clips_against_an_offset_clip_rect() {
    let clip = geom::Rect::new(5, 2, 10, 5);
    let cases = [
        TextCase {
            name: "text starts before the clip",
            clip,
            line: line(0, 2, 15),
            text: "01234567890123456789",
            expected: buf!(
                "5678901234"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        },
        TextCase {
            name: "text extends past the clip",
            clip,
            line: line(10, 3, 10),
            text: "01234567890",
            expected: buf!(
                "XXXXXXXXXX"
                "XXXXX01234"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        },
        TextCase {
            name: "text inside the clip",
            clip,
            line: line(7, 3, 5),
            text: "Hello",
            expected: buf!(
                "XXXXXXXXXX"
                "XXHelloXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        },
    ];
    for case in &cases {
        case.run();
    }
}

#[test]
fn test_part_render_multiple_rectangles() {
    // Clipping is relative to the render rect, wherever it sits on the canvas.
    let positions = [
        geom::Rect::new(0, 0, 10, 10),
        geom::Rect::new(10, 10, 10, 10),
        geom::Rect::new(20, 20, 10, 10),
    ];

    for render_rect in positions {
        let mut target = TestTarget::new(render_rect);

        // Fill within the specific render rect, then outside the canvas.
        let fill_rect = geom::Rect::new(render_rect.tl.x + 1, render_rect.tl.y + 1, 5, 5);
        target
            .render(|r| r.fill("default", fill_rect, '#'))
            .unwrap();
        target
            .render(|r| r.fill("default", geom::Rect::new(40, 40, 5, 5), 'Y'))
            .unwrap();

        target.assert_matches(buf!(
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
        ));
    }
}
