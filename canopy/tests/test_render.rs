use canopy::{
    Expanse, Render, TermBuf, ViewPort, geom,
    style::{AttrSet, Color, Style, StyleManager, StyleMap},
};

fn assert_buffer_matches(buf: &TermBuf, expected: &[&str]) {
    buf.assert_matches(expected);
}

fn setup_render_test(
    buf_size: Expanse,
    viewport: ViewPort,
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
    (buf, stylemap, style_manager, viewport)
}

struct BufTest {
    name: &'static str,
    line: geom::Line,
    text: &'static str,
    expected: &'static [&'static str],
    viewport: Option<ViewPort>,
}

macro_rules! buf {
    ($($line:literal)*) => {
        &[$($line),*]
    };
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
            viewport: None,
        }
    }

    fn buffer_size(&self) -> Expanse {
        assert!(
            !self.expected.is_empty(),
            "Cannot calculate buffer size from empty expected buffer"
        );
        Expanse::new(self.expected[0].len() as u16, self.expected.len() as u16)
    }

    fn line(mut self, x: u16, y: u16, width: u16, text: &'static str) -> Self {
        self.line = geom::Line {
            tl: geom::Point { x, y },
            w: width,
        };
        self.text = text;
        self
    }

    fn vp(mut self, viewport: ViewPort) -> Self {
        self.viewport = Some(viewport);
        self
    }

    fn run(self) {
        let buf_size = self.buffer_size();
        let viewport = self.viewport.unwrap_or_else(|| {
            ViewPort::new(
                buf_size,
                geom::Rect::new(0, 0, buf_size.w, buf_size.h),
                geom::Point::zero(),
            )
            .unwrap()
        });
        let (mut buf, stylemap, mut style_manager, viewport) =
            setup_render_test(buf_size, viewport);

        let base = geom::Point::zero();
        let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

        render.text("default", self.line, self.text).unwrap();

        // Use the new assert_matches method which includes pretty printing
        let context = format!(
            "=== Test case '{}' failed ===\nText: '{}' at line({},{}) width={}",
            self.name, self.text, self.line.tl.x, self.line.tl.y, self.line.w
        );
        buf.assert_matches_with_context(self.expected, Some(&context));
    }
}

#[test]
fn test_fill_full_viewport() {
    let buf_size = Expanse::new(10, 5);
    let viewport =
        ViewPort::new(buf_size, geom::Rect::new(0, 0, 10, 5), geom::Point::zero()).unwrap();
    let (mut buf, stylemap, mut style_manager, viewport) = setup_render_test(buf_size, viewport);

    let base = geom::Point::zero();
    let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

    // Fill a rectangle in the middle of the buffer
    let rect = geom::Rect::new(2, 1, 4, 2);
    render.fill("default", rect, '#').unwrap();

    // Check that the rectangle was filled correctly
    assert_buffer_matches(
        &buf,
        &[
            "          ",
            "  ####    ",
            "  ####    ",
            "          ",
            "          ",
        ],
    );
}

#[test]
fn test_fill_with_base_offset() {
    let buf_size = Expanse::new(10, 5);
    let viewport =
        ViewPort::new(buf_size, geom::Rect::new(0, 0, 10, 5), geom::Point::zero()).unwrap();
    let (mut buf, stylemap, mut style_manager, viewport) = setup_render_test(buf_size, viewport);

    let base = (1, 1).into();
    let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

    // Fill a rectangle at (0,0) which should appear at (1,1) due to base offset
    let rect = geom::Rect::new(0, 0, 3, 2);
    render.fill("default", rect, 'X').unwrap();

    // Check that the rectangle was filled at the offset position
    assert_buffer_matches(
        &buf,
        &[
            "          ",
            " XXX      ",
            " XXX      ",
            "          ",
            "          ",
        ],
    );
}

#[test]
fn test_text_rendering() {
    let v = ViewPort::new(
        Expanse::new(5, 5),
        geom::Rect::new(0, 0, 5, 5),
        geom::Point::zero(),
    )
    .unwrap();

    let test_cases = vec![
        BufTest::new(
            "full line",
            buf!(
                "     "
                "Hello"
                "     "
                "     "
                "     "
            ),
        )
        .line(0, 1, 5, "Hello")
        .vp(v),
        BufTest::new(
            "overflow",
            buf!(
                "Hello"
                "     "
                "     "
                "     "
                "     "
            ),
        )
        .line(0, 0, 5, "Hello World")
        .vp(v),
        BufTest::new(
            "truncation",
            buf!(
                "He   "
                "     "
                "     "
                "     "
                "     "
            ),
        )
        .line(0, 0, 2, "Hello World")
        .vp(v),
        BufTest::new(
            "truncation - 0",
            buf!(
                "     "
                "     "
                "     "
                "     "
                "     "
            ),
        )
        .line(0, 0, 0, "Hello World")
        .vp(v),
        BufTest::new(
            "with padding",
            buf!(
                "     "
                "     "
                "Hi   "
                "     "
                "     "
            ),
        )
        .line(0, 2, 5, "Hi")
        .vp(v),
        BufTest::new(
            "out of bounds bottom",
            buf!(
                "     "
                "     "
                "     "
                "     "
                "     "
            ),
        )
        .line(0, 10, 5, "Hi")
        .vp(v),
        BufTest::new(
            "out of bounds right",
            buf!(
                "     "
                "     "
                "     "
                "     "
                "     "
            ),
        )
        .line(10, 0, 5, "Hi")
        .vp(v),
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
    let buf_size = Expanse::new(10, 10);
    let viewport =
        ViewPort::new(buf_size, geom::Rect::new(0, 0, 10, 10), geom::Point::zero()).unwrap();
    let (mut buf, stylemap, mut style_manager, viewport) = setup_render_test(buf_size, viewport);

    let base = geom::Point::zero();
    let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

    // Create a frame around a 6x6 area starting at (2,2)
    let frame = geom::Frame::new(geom::Rect::new(2, 2, 6, 6), 1);
    render.solid_frame("default", frame, '*').unwrap();

    // Check the frame is drawn correctly
    assert_buffer_matches(
        &buf,
        &[
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
        ],
    );
}

#[test]
fn test_solid_frame_single_width() {
    let buf_size = Expanse::new(5, 5);
    let viewport =
        ViewPort::new(buf_size, geom::Rect::new(0, 0, 5, 5), geom::Point::zero()).unwrap();
    let (mut buf, stylemap, mut style_manager, viewport) = setup_render_test(buf_size, viewport);

    let base = geom::Point::zero();
    let mut render = Render::new(&mut buf, &stylemap, &mut style_manager, viewport, base);

    // Create a minimal frame
    let frame = geom::Frame::new(geom::Rect::new(1, 1, 3, 3), 1);
    render.solid_frame("default", frame, '#').unwrap();

    // Check that frame is drawn correctly
    assert_buffer_matches(&buf, &["     ", " ### ", " # # ", " ### ", "     "]);
}
