use canopy::{
    Expanse, buf, geom,
    render::Render,
    style::{StyleManager, StyleMap},
    tutils::buf as tutils_buf,
};

fn assert_buffer_matches(render: &Render, expected: &[&str]) {
    tutils_buf::assert_matches(render.get_buffer(), expected);
}

fn setup_render_test(_canvas_size: Expanse, _render_rect: geom::Rect) -> (StyleMap, StyleManager) {
    let stylemap = StyleMap::new();
    // The default style is already added by StyleMap::new()

    let style_manager = StyleManager::default();
    (stylemap, style_manager)
}

struct BufTest {
    name: &'static str,
    line: geom::Line,
    text: &'static str,
    expected: &'static [&'static str],
    canvas_size: Option<Expanse>,
    render_rect: Option<geom::Rect>,
}

impl BufTest {
    fn new(name: &'static str, expected: &'static [&'static str]) -> Self {
        assert!(
            !expected.is_empty(),
            "Test case '{name}': expected buffer cannot be empty",
        );

        let first_line_len = expected[0].len();
        for (i, line) in expected.iter().enumerate() {
            assert_eq!(
                line.len(),
                first_line_len,
                "Test case '{}': line {} has length {}, but expected length {} (same as line 0)",
                name,
                i,
                line.len(),
                first_line_len
            );
        }

        Self {
            name,
            line: geom::Line {
                tl: geom::Point { x: 0, y: 0 },
                w: 1,
            },
            text: "",
            expected,
            canvas_size: None,
            render_rect: None,
        }
    }

    fn buffer_size(&self) -> Expanse {
        assert!(
            !self.expected.is_empty(),
            "Cannot calculate buffer size from empty expected buffer"
        );
        Expanse::new(self.expected[0].len() as u32, self.expected.len() as u32)
    }

    fn line(mut self, x: u32, y: u32, width: u32, text: &'static str) -> Self {
        self.line = geom::Line {
            tl: geom::Point { x, y },
            w: width,
        };
        self.text = text;
        self
    }

    fn canvas(mut self, canvas_size: Expanse) -> Self {
        self.canvas_size = Some(canvas_size);
        self
    }

    fn render_rect(mut self, rect: geom::Rect) -> Self {
        self.render_rect = Some(rect);
        self
    }

    fn run(self) {
        let buf_size = self.buffer_size();
        let canvas_size = self.canvas_size.unwrap_or(buf_size);
        let render_rect = self
            .render_rect
            .unwrap_or_else(|| geom::Rect::new(0, 0, buf_size.w, buf_size.h));

        let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
        let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

        let result = render.text("default", self.line, self.text);
        if let Err(e) = result {
            panic!("Text rendering failed for test '{}': {:?}", self.name, e);
        }

        // Use the new assert_matches method which includes pretty printing
        let context = format!(
            "=== Test case '{}' failed ===\nText: '{}' at line({},{}) width={}\nCanvas: {}x{}, Render rect: ({},{}) {}x{}",
            self.name,
            self.text,
            self.line.tl.x,
            self.line.tl.y,
            self.line.w,
            canvas_size.w,
            canvas_size.h,
            render_rect.tl.x,
            render_rect.tl.y,
            render_rect.w,
            render_rect.h
        );
        tutils_buf::assert_matches_with_context(render.get_buffer(), self.expected, Some(&context));
    }
}

#[test]
fn test_fill_full_render_rect() {
    let canvas_size = Expanse::new(10, 5);
    let render_rect = geom::Rect::new(0, 0, 10, 5);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Fill a rectangle in the middle of the buffer
    let rect = geom::Rect::new(2, 1, 4, 2);
    render.fill("default", rect, '#').unwrap();

    // Check that the rectangle was filled correctly
    assert_buffer_matches(
        &render,
        &[
            "XXXXXXXXXX",
            "XX####XXXX",
            "XX####XXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
        ],
    );
}

#[test]
fn test_fill_partial_render_rect() {
    let canvas_size = Expanse::new(20, 10);
    let render_rect = geom::Rect::new(5, 2, 10, 5);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Fill a rectangle that partially overlaps the render rect
    // Rectangle at (3, 1) with size 10x5 should only render the part that overlaps with render_rect
    let rect = geom::Rect::new(3, 1, 10, 5);
    render.fill("default", rect, '#').unwrap();

    // The render buffer starts at (5,2) and is 10x5
    // The fill starts at (3,1) and goes to (12,5)
    // y=0 in buffer is y=2 in canvas, fill rect starts at y=1, so first row should show fill
    assert_buffer_matches(
        &render,
        &[
            "########XX", // y=2 in canvas (y=0 in buffer): fill from x=5 to x=12 (0-7 in buffer)
            "########XX", // y=3 in canvas (y=1 in buffer): fill from x=5 to x=12 (0-7 in buffer)
            "########XX", // y=4 in canvas (y=2 in buffer): fill from x=5 to x=12 (0-7 in buffer)
            "########XX", // y=5 in canvas (y=3 in buffer): fill from x=5 to x=12 (0-7 in buffer)
            "XXXXXXXXXX", // y=6 in canvas (y=4 in buffer): fill rect ends at y=5
        ],
    );
}

#[test]
fn test_text_rendering() {
    let canvas = Expanse::new(20, 10);
    let render_rect = geom::Rect::new(0, 0, 5, 5);

    let test_cases = vec![
        BufTest::new(
            "full line",
            buf!(
                "XXXXX"
                "Hello"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 1, 5, "Hello")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "overflow",
            buf!(
                "Hello"
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 0, 5, "Hello World")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "truncation",
            buf!(
                "HeXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 0, 2, "Hello World")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "truncation - 0",
            buf!(
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 0, 0, "Hello World")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "with padding",
            buf!(
                "XXXXX"
                "XXXXX"
                "Hi   "
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 2, 5, "Hi")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "out of bounds bottom",
            buf!(
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(0, 5, 5, "Hi") // Changed from y=10 to y=5 to be within canvas bounds
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "out of bounds right",
            buf!(
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
                "XXXXX"
            ),
        )
        .line(10, 0, 5, "Hi")
        .canvas(canvas)
        .render_rect(render_rect),
    ];

    for test_case in test_cases {
        test_case.run();
    }
}

#[test]
fn test_text_partial_overlap() {
    let canvas = Expanse::new(20, 10);
    let render_rect = geom::Rect::new(5, 2, 10, 5);

    let test_cases = vec![
        BufTest::new(
            "text starts before render rect",
            buf!(
                "5678901234"  // Text line width is 15, so it shows chars 5-14
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        )
        .line(0, 2, 15, "01234567890123456789")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "text extends beyond render rect",
            buf!(
                "XXXXXXXXXX"
                "XXXXX01234"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        )
        .line(10, 3, 10, "01234567890")
        .canvas(canvas)
        .render_rect(render_rect),
        BufTest::new(
            "text completely within render rect",
            buf!(
                "XXXXXXXXXX"
                "XXHelloXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
                "XXXXXXXXXX"
            ),
        )
        .line(7, 3, 5, "Hello")
        .canvas(canvas)
        .render_rect(render_rect),
    ];

    for test_case in test_cases {
        test_case.run();
    }
}

#[test]
fn test_buffer_macro_flexibility() {
    // Test that the buffer macro works with different line counts

    // 2x2 buffer
    let small = buf!(
        "ab"
        "cd"
    );
    assert_eq!(small.len(), 2);
    assert_eq!(small[0], "ab");
    assert_eq!(small[1], "cd");

    // 3x4 buffer
    let medium = buf!(
        "test"
        "more"
        "text"
    );
    assert_eq!(medium.len(), 3);
    assert_eq!(medium[0], "test");
    assert_eq!(medium[1], "more");
    assert_eq!(medium[2], "text");

    // 1x10 buffer (single line)
    let single = buf!("single line");
    assert_eq!(single.len(), 1);
    assert_eq!(single[0], "single line");
}

#[test]
fn test_solid_frame() {
    let canvas_size = Expanse::new(10, 10);
    let render_rect = geom::Rect::new(0, 0, 10, 10);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Create a frame around a 6x6 area starting at (2,2)
    let frame = geom::Frame::new(geom::Rect::new(2, 2, 6, 6), 1);
    render.solid_frame("default", frame, '*').unwrap();

    // Check the frame is drawn correctly
    assert_buffer_matches(
        &render,
        &[
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XX******XX",
            "XX*XXXX*XX",
            "XX*XXXX*XX",
            "XX*XXXX*XX",
            "XX*XXXX*XX",
            "XX******XX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
        ],
    );
}

#[test]
fn test_solid_frame_single_width() {
    let canvas_size = Expanse::new(5, 5);
    let render_rect = geom::Rect::new(0, 0, 5, 5);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Create a minimal frame
    let frame = geom::Frame::new(geom::Rect::new(1, 1, 3, 3), 1);
    render.solid_frame("default", frame, '#').unwrap();

    // Check that frame is drawn correctly
    assert_buffer_matches(&render, &["XXXXX", "X###X", "X#X#X", "X###X", "XXXXX"]);
}

#[test]
fn test_solid_frame_partial_overlap() {
    let canvas_size = Expanse::new(20, 15);
    let render_rect = geom::Rect::new(5, 5, 10, 5);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Create a frame that partially overlaps the render rect
    let frame = geom::Frame::new(geom::Rect::new(3, 3, 10, 8), 1);
    render.solid_frame("default", frame, '#').unwrap();

    // The render rect starts at (5,5) and is 10x5
    // The frame is at (3,3) to (12,10), with border 1
    // Frame parts: top (3,3,10,1), bottom (3,10,10,1), left (3,4,1,6), right (12,4,1,6)
    // Only parts that overlap with render rect (5,5,10,5) will be visible
    assert_buffer_matches(
        &render,
        &[
            "XXXXXXX#XX", // y=5: right edge at x=12
            "XXXXXXX#XX", // y=6: right edge at x=12
            "XXXXXXX#XX", // y=7: right edge at x=12
            "XXXXXXX#XX", // y=8: right edge at x=12
            "XXXXXXX#XX", // y=9: right edge at x=12
        ],
    );
}

#[test]
fn test_multiple_render_rects() {
    let canvas_size = Expanse::new(30, 20);

    // Test different render rect positions within the same canvas
    let positions = vec![
        (geom::Rect::new(0, 0, 10, 10), "top-left"),
        (geom::Rect::new(10, 5, 10, 10), "middle"),
        (geom::Rect::new(20, 10, 10, 10), "bottom-right"),
    ];

    for (render_rect, position) in positions {
        let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
        let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

        // Fill the entire render rect with a pattern
        render.fill("default", render_rect, '.').unwrap();

        // Draw a smaller rectangle in the middle of the render rect
        let inner_rect = geom::Rect::new(render_rect.tl.x + 2, render_rect.tl.y + 2, 6, 6);
        render.fill("default", inner_rect, '#').unwrap();

        // Verify the pattern
        let expected = &[
            "..........",
            "..........",
            "..######..",
            "..######..",
            "..######..",
            "..######..",
            "..######..",
            "..######..",
            "..........",
            "..........",
        ];

        let context = format!("Testing render rect at position: {position}");
        tutils_buf::assert_matches_with_context(render.get_buffer(), expected, Some(&context));
    }
}

#[test]
fn test_render_outside_canvas_bounds() {
    let canvas_size = Expanse::new(20, 20);
    let render_rect = geom::Rect::new(5, 5, 10, 10);
    let (stylemap, mut style_manager) = setup_render_test(canvas_size, render_rect);
    let mut render = Render::new(&stylemap, &mut style_manager, canvas_size, render_rect);

    // Try to fill a rectangle that extends outside the canvas
    let result = render.fill("default", geom::Rect::new(15, 15, 10, 10), '#');
    assert!(result.is_err());

    // Try to render text that extends outside the canvas
    let result = render.text(
        "default",
        geom::Line {
            tl: geom::Point { x: 18, y: 18 },
            w: 5,
        },
        "Text",
    );
    assert!(result.is_err());

    // The buffer should remain unchanged (all NULL)
    assert_buffer_matches(
        &render,
        &[
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
            "XXXXXXXXXX",
        ],
    );
}
