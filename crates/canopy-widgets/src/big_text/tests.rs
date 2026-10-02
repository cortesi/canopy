//! Tests of `BigText`.

use std::sync::Arc;

use canopy::{
    ContextExt, NodeId,
    geom::Point,
    layout::{Edges, LayoutOverride},
    style::{Color, ResolvedStyle},
    testing::{ManualClock, harness::Harness},
};

use super::{
    face::BigWeight,
    layout::{Column, Laid},
    *,
};
use crate::Scroll;

/// Every character that each face and weight draws, apart from the space.
const REPERTOIRE: &str = concat!(
    "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`",
    "abcdefghijklmnopqrstuvwxyz{|}~",
    "¡¢£¤¥¦¨©«°´¸»¿ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ",
    "─│┌┐└┘├┤┬┴┼▒",
    "×÷≤≥°·—…↑↓▲▼±µ€",
);

/// The digits of the compact face, as their cell rows.
const DIGITS: [&str; 3] = [
    "█▀█ ▄█  ▀▀█ ▀▀█ █ █ █▀▀ █▀▀ ▀▀█ █▀█ █▀█",
    "█ █  █  █▀▀ ▀▀█ ▀▀█ ▀▀█ █▀█   █ █▀█ ▀▀█",
    "▀▀▀ ▀▀▀ ▀▀▀ ▀▀▀   ▀ ▀▀▀ ▀▀▀   ▀ ▀▀▀ ▀▀▀",
];

/// An area large enough for every test text.
const ROOMY: Size = Size { w: 1000, h: 1000 };

/// Returns text in the compact face at scale 1.
fn compact(text: &str) -> BigText {
    BigText::new(text).with_size(BigSize::Exact {
        face: BigFace::Compact,
        scale: 1,
    })
}

/// Returns the cell rows of text in the compact face at scale 1.
fn rows_of(text: &str) -> Vec<String> {
    compact(text).rows_in(ROOMY)
}

/// Returns the size of text in the compact face at scale 1.
fn size_of(text: &str) -> Size {
    compact(text).size_in(ROOMY)
}

#[test]
fn every_face_and_weight_draws_the_repertoire() {
    for face in BigFace::ALL {
        for weight in [BigWeight::Bold, BigWeight::Regular] {
            let unknown = face.glyph(weight, '?');
            for ch in REPERTOIRE.chars() {
                let glyph = face.glyph(weight, ch);
                assert!(glyph.has_ink(), "{face:?} {weight:?} draws {ch}");
                assert!(
                    ch == '?' || glyph != unknown,
                    "{face:?} {weight:?} has {ch}"
                );
            }
        }
    }
}

#[test]
fn digits_and_capitals_stand_on_one_baseline_at_the_cap_height() {
    for face in BigFace::ALL {
        for weight in [BigWeight::Bold, BigWeight::Regular] {
            for ch in "0123456789HKX".chars() {
                let glyph = face.glyph(weight, ch);
                assert_eq!(
                    u32::try_from(glyph.top).ok(),
                    Some(face.cap_height()),
                    "{face:?} {weight:?} {ch} reaches the cap height"
                );
                assert_eq!(
                    glyph.descent(),
                    0,
                    "{face:?} {weight:?} {ch} sits on the baseline"
                );
            }
        }
    }
}

#[test]
fn compact_digits_draw_in_three_rows_of_half_blocks() {
    assert_eq!(rows_of("0123456789"), DIGITS);
    assert_eq!(size_of("0123456789"), Size::new(39, 3));
    assert_eq!(size_of(""), Size::new(0, 3), "empty text keeps one line");
}

#[test]
fn lowercase_draws_in_lowercase_and_unknown_characters_as_a_question() {
    assert_ne!(rows_of("abc"), rows_of("ABC"));
    assert_eq!(rows_of("ж"), rows_of("?"));
    assert_eq!(rows_of("4−2"), rows_of("4-2"));
    assert_eq!(rows_of("a\tb"), rows_of("a b"));
}

#[test]
fn narrow_glyphs_keep_their_widths() {
    assert_eq!(size_of("1.5"), Size::new(3 + 1 + 1 + 1 + 3, 3));
    assert_eq!(size_of("1"), Size::new(3, 3));
}

#[test]
fn a_newline_starts_a_line_below_a_blank_row() {
    assert_eq!(size_of("12\n3"), Size::new(7, 7));
    let rows = rows_of("1\n2");
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[3], "", "a blank row between the lines");
    assert_eq!(rows[4], rows_of("2")[0]);
}

#[test]
fn a_fit_draws_the_largest_capitals_at_the_lowest_scale() {
    let fit = |text: &str, w, h| BigText::new(text).face_in(Size::new(w, h));
    // Three rows take the compact face: it is narrower than Tamzen 5×9.
    assert_eq!(fit("123", 40, 3), (BigFace::Compact, 1));
    // Five rows take Tamzen 10×20 at scale 1 over the compact face at scale
    // 2, with capitals of the same size, and over the smaller Tamzen 8×16.
    assert_eq!(fit("123", 200, 5), (BigFace::Tamzen10x20, 1));
    // A narrow area takes a smaller rendering.
    assert_eq!(fit("12345", 20, 5), (BigFace::Compact, 1));
    // A row that larger capitals do not use stays empty: the percent sign
    // of Tamzen 5×9 takes a fourth row, but its digits are no larger.
    assert_eq!(fit("1%", 10, 4), (BigFace::Compact, 1));
    // Scale fills a tall area, at the lowest scale that fills it.
    assert_eq!(fit("1", 200, 10), (BigFace::Tamzen10x20, 2));
    assert_eq!(fit("1", 200, 8), (BigFace::Tamzen7x14, 2));
}

#[test]
fn a_dash_a_dot_or_a_space_fits_like_a_digit() {
    for area in [Size::new(40, 3), Size::new(200, 5), Size::new(200, 12)] {
        let digit = BigText::new("0").face_in(area);
        for text in ["—", ".", " ", ""] {
            assert_eq!(
                BigText::new(text).face_in(area),
                digit,
                "{text:?} in {area:?}"
            );
        }
    }
}

#[test]
fn descenders_take_rows_below_the_baseline() {
    assert_eq!(size_of("Ag").h, 4);
    assert_eq!(size_of("AB").h, 3);
    // So four rows fit descenders in the compact face, and capitals in a
    // larger one.
    let area = Size::new(200, 4);
    assert_eq!(BigText::new("Ag").face_in(area), (BigFace::Compact, 1));
    assert_eq!(BigText::new("AB").face_in(area), (BigFace::Tamzen7x14, 1));
}

#[test]
fn a_strut_grows_the_line_box_and_draws_nothing() {
    let strut = compact("12").with_strut("g");
    assert_eq!(
        strut.size_in(ROOMY),
        Size::new(size_of("12").w, size_of("12g").h)
    );
    let mut rows = rows_of("12");
    rows.push(String::new());
    assert_eq!(strut.rows_in(ROOMY), rows, "the digits keep their rows");
    // A fit takes the strut as if the text had it.
    let area = Size::new(200, 5);
    assert_eq!(BigText::new("12").face_in(area), (BigFace::Tamzen10x20, 1));
    assert_eq!(
        BigText::new("12").with_strut("k").face_in(area),
        BigText::new("12k").face_in(area)
    );
}

#[test]
fn nothing_fits_takes_the_smallest_rendering() {
    assert_eq!(
        BigText::new("12345").face_in(Size::new(2, 1)),
        (BigFace::Compact, 1)
    );
}

#[test]
fn max_rows_caps_a_fit_and_exact_takes_its_size() {
    let capped = BigText::new("1").with_size(BigSize::MaxRows(3));
    assert_eq!(capped.face_in(Size::new(200, 20)), (BigFace::Compact, 1));
    let exact = BigText::new("1").with_size(BigSize::Exact {
        face: BigFace::Tamzen6x12,
        scale: 0,
    });
    assert_eq!(exact.face_in(Size::new(1, 1)), (BigFace::Tamzen6x12, 1));
}

#[test]
fn measure_follows_the_constraints() {
    let measure =
        |text: BigText, width, height| match text.measure(MeasureConstraints { width, height }) {
            Measurement::Fixed(size) => size,
            Measurement::Wrap => panic!("big text has its own size"),
        };
    // Without a bound on the height, a fit takes the compact face.
    assert_eq!(
        measure(
            BigText::new("123"),
            Constraint::Unbounded,
            Constraint::Unbounded
        ),
        Size::new(11, 3)
    );
    // With bounds, a fit fills the area, where it draws the rendering that
    // the area allows.
    assert_eq!(
        measure(
            BigText::new("1"),
            Constraint::AtMost(200),
            Constraint::AtMost(10)
        ),
        Size::new(200, 10)
    );
    assert_eq!(BigText::new("1").size_in(Size::new(200, 10)).h, 10);
    // A row limit applies without a bound on the height, and a capped fit
    // fills a bounded width.
    let capped = || BigText::new("1").with_size(BigSize::MaxRows(5));
    assert_eq!(
        measure(capped(), Constraint::Unbounded, Constraint::Unbounded).h,
        5
    );
    assert_eq!(
        measure(capped(), Constraint::AtMost(40), Constraint::Unbounded),
        Size::new(40, 5)
    );
    // An exact size measures alike under any bound.
    let exact = || {
        BigText::new("10").with_size(BigSize::Exact {
            face: BigFace::Compact,
            scale: 2,
        })
    };
    assert_eq!(
        measure(exact(), Constraint::Unbounded, Constraint::Unbounded),
        Size::new(14, 5)
    );
    assert_eq!(
        measure(exact(), Constraint::AtMost(100), Constraint::AtMost(100)),
        Size::new(14, 5)
    );
}

#[test]
fn a_scale_multiplies_columns_and_half_rows() {
    let scaled = |scale| {
        BigText::new("0").with_size(BigSize::Exact {
            face: BigFace::Compact,
            scale,
        })
    };
    assert_eq!(
        scaled(2).rows_in(ROOMY),
        ["██████", "██  ██", "██  ██", "██  ██", "██████"]
    );
    // At an odd scale, pixel rows end in the middle of a cell.
    assert_eq!(scaled(3).size_in(ROOMY), Size::new(9, 8));
    let rows = scaled(3).rows_in(ROOMY);
    assert_eq!(rows[0], "█████████");
    assert_eq!(rows[1], "███▀▀▀███", "a cell holds two pixel rows");
    assert_eq!(rows[7], "▀▀▀▀▀▀▀▀▀", "the bottom row ends at a half row");
}

/// Unit color of the styled runs test.
const UNIT: Color = Color::Rgb {
    r: 90,
    g: 90,
    b: 90,
};

/// Returns the glyph and the style of one cell.
fn cell_at(harness: &Harness, x: u32, y: u32) -> (char, ResolvedStyle) {
    let cell = harness.buf().get(Point { x, y }).expect("cell");
    (cell.ch, cell.style)
}

#[test]
fn runs_paint_in_their_own_styles() -> Result<()> {
    let text = BigText::new("").with_runs([("text", "4"), ("unit", "%")]);
    let mut harness = Harness::builder(text)
        .size(12, 3)
        .configure(|setup| {
            setup.widget_styles(|_palette, rules| {
                rules.fg("big_text/unit", UNIT).apply();
            });
            Ok(())
        })
        .build()?;
    harness.render()?;
    let lines = harness.tbuf().lines();
    assert_eq!(lines[0].trim_end(), "█ █ ▀ █");
    let (glyph, digit) = cell_at(&harness, 0, 0);
    assert_eq!(glyph, '█');
    assert_ne!(digit.fg, UNIT, "the digit takes the text part");
    let (glyph, unit) = cell_at(&harness, 4, 0);
    assert_eq!(glyph, '▀');
    assert_eq!(unit.fg, UNIT, "the unit takes its own style");
    Ok(())
}

#[test]
fn text_stays_within_its_area() -> Result<()> {
    // "88" is seven columns wide and three rows high, but the widget has
    // only five columns and two rows inside its padding, so the smallest
    // rendering shows, cut at the area.
    let mut harness = Harness::builder(Padded(Some(BigText::new("88"))))
        .size(7, 4)
        .build()?;
    harness.render()?;
    let lines = harness.tbuf().lines();
    assert_eq!(lines[1], " █▀█ █ ", "the right padding stays blank");
    assert_eq!(lines[2], " █▀█ █ ");
    assert_eq!(lines[3], "       ", "the bottom padding stays blank");
    Ok(())
}

/// A root that holds its child inside one cell of padding.
struct Padded(Option<BigText>);

impl Widget for Padded {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("", ctx.view().view_rect_local(), ' ')
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let child = self.0.take().expect("child");
        let node = ctx.add_child(ctx.node_id(), child)?;
        let padded = LayoutOverride {
            padding: Some(Edges::all(1)),
            ..LayoutOverride::new().flex_horizontal(1).flex_vertical(1)
        };
        ctx.set_layout_override(node.into(), padded)
    }
}

#[test]
fn lines_align_in_a_wider_area() -> Result<()> {
    let text = BigText::new("1\n11").with_align(Align::End);
    let mut harness = Harness::builder(text).size(9, 7).build()?;
    harness.render()?;
    let lines = harness.tbuf().lines();
    assert_eq!(lines[0].trim_end(), "      ▄█");
    assert_eq!(lines[4].trim_end(), "  ▄█  ▄█");
    Ok(())
}

#[test]
fn the_block_centers_vertically_in_half_rows() -> Result<()> {
    let text = compact("0").with_vertical_align(Align::Center);
    let mut harness = Harness::builder(text).size(3, 4).build()?;
    harness.render()?;
    // Five half rows in eight leave three: one above, two below.
    let lines = harness.tbuf().lines();
    assert_eq!(lines, ["▄▄▄", "█ █", "█▄█", "   "]);
    Ok(())
}

/// Returns a harness of `text` whose clock the test moves, and the clock.
fn clocked(text: &str, motion: bool) -> Result<(Harness, Arc<ManualClock>)> {
    let clock = Arc::new(ManualClock::new());
    let mut harness = Harness::builder(BigText::new(text))
        .size(20, 3)
        .clock(Arc::clone(&clock))
        .motion(motion)
        .build()?;
    harness.render()?;
    Ok((harness, clock))
}

/// Returns the rows on screen, without trailing spaces.
fn screen(harness: &Harness) -> Vec<String> {
    harness
        .tbuf()
        .lines()
        .iter()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// Sets the text of the root widget.
fn set(harness: &mut Harness, text: &str) {
    harness.with_root_widget(|widget: &mut BigText| widget.set_text(text));
}

/// Returns whether the root widget rolls.
fn rolls(harness: &mut Harness) -> bool {
    harness.with_root_widget(|widget: &mut BigText| widget.rolling())
}

/// Moves the clock on and renders.
fn after(harness: &mut Harness, clock: &ManualClock, millis: u64) -> Result<()> {
    clock.advance(Duration::from_millis(millis))?;
    harness.render()
}

#[test]
fn a_changed_character_rolls_and_the_others_stay() -> Result<()> {
    let (mut harness, clock) = clocked("12", true)?;
    set(&mut harness, "13");
    harness.render()?;
    assert_eq!(screen(&harness), rows_of("12"), "the roll starts at rest");
    assert!(rolls(&mut harness));
    after(&mut harness, &clock, 80)?;
    let middle = screen(&harness);
    let (old, new) = (rows_of("12"), rows_of("13"));
    for row in 0..3 {
        let prefix = |text: &str| text.chars().take(4).collect::<String>();
        assert_eq!(prefix(&middle[row]), prefix(&new[row]), "the 1 stays");
    }
    assert_ne!(middle, old);
    assert_ne!(middle, new);
    after(&mut harness, &clock, 120)?;
    assert_eq!(screen(&harness), new);
    assert!(!rolls(&mut harness), "the roll ends at 200 ms");
    Ok(())
}

/// Returns the pixel columns of one line in the compact face.
fn line_columns(text: &str) -> Vec<Column> {
    let laid = Laid::new(
        BigFace::Compact,
        BigWeight::Bold,
        &[text.chars().map(|ch| (0, ch)).collect()],
        "",
    );
    laid.columns(&laid.lines[0])
}

/// Returns the cell rows of one line of five pixel rows.
fn column_rows(columns: &[Column]) -> Vec<String> {
    (0..3)
        .map(|row| {
            let text: String = columns
                .iter()
                .map(
                    |column| match (column.ink(row * 2), column.ink(row * 2 + 1)) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    },
                )
                .collect();
            text.trim_end().to_owned()
        })
        .collect()
}

#[test]
fn the_roll_moves_the_old_glyph_up_and_the_new_one_in_from_below() {
    let from = line_columns("8");
    let to = line_columns("1");
    let rows = |progress| column_rows(&roll_line(&from, &to, progress, 5));
    assert_eq!(rows(0.0), rows_of("8"));
    assert_eq!(rows(0.05), rows_of("8"), "less than one pixel row");
    // Two pixel rows up: the bottom three rows of the eight, a blank row,
    // and the top row of the one.
    assert_eq!(rows(0.15), ["█▀█", "▀▀▀", " ▀"]);
    assert_eq!(rows(1.0), rows_of("1"));
}

#[test]
fn a_change_during_a_roll_starts_from_the_glyphs_on_screen() -> Result<()> {
    let (mut harness, clock) = clocked("12", true)?;
    set(&mut harness, "13");
    harness.render()?;
    after(&mut harness, &clock, 60)?;
    let middle = screen(&harness);
    set(&mut harness, "14");
    harness.render()?;
    assert_eq!(screen(&harness), middle, "the new roll starts on screen");
    after(&mut harness, &clock, 150)?;
    assert!(rolls(&mut harness), "the new roll has its own 200 ms");
    after(&mut harness, &clock, 50)?;
    assert_eq!(screen(&harness), rows_of("14"));
    assert!(!rolls(&mut harness));
    Ok(())
}

#[test]
fn a_longer_text_aligns_on_the_right_and_rolls_in_on_the_left() -> Result<()> {
    let (mut harness, clock) = clocked("9", true)?;
    set(&mut harness, "10");
    harness.render()?;
    let shifted = rows_of("9")
        .iter()
        .map(|row| format!("    {row}"))
        .collect::<Vec<_>>();
    assert_eq!(
        screen(&harness),
        shifted,
        "the old digit moves to the right"
    );
    after(&mut harness, &clock, 200)?;
    assert_eq!(screen(&harness), rows_of("10"));
    Ok(())
}

#[test]
fn a_change_of_geometry_shows_at_once() -> Result<()> {
    let (mut harness, _clock) = clocked("12", true)?;
    harness.with_root_widget(|widget: &mut BigText| {
        widget.set_text("13");
        widget.set_size(BigSize::Exact {
            face: BigFace::Tamzen5x9,
            scale: 1,
        });
    });
    harness.render()?;
    assert!(!rolls(&mut harness), "another face does not roll");
    let expected = BigText::new("13")
        .with_size(BigSize::Exact {
            face: BigFace::Tamzen5x9,
            scale: 1,
        })
        .rows_in(Size::new(20, 3));
    assert_eq!(screen(&harness), expected);
    Ok(())
}

#[test]
fn a_strut_lets_a_taller_glyph_roll_in() -> Result<()> {
    let (mut harness, _clock) = clocked("12", true)?;
    set(&mut harness, "1k");
    harness.render()?;
    assert!(!rolls(&mut harness), "the k grows the line box");
    let (mut harness, _clock) = clocked("12", true)?;
    harness.with_root_widget(|widget: &mut BigText| widget.set_strut("k"));
    harness.render()?;
    set(&mut harness, "1k");
    harness.render()?;
    assert!(rolls(&mut harness), "the line box holds the k already");
    Ok(())
}

#[test]
fn motion_that_turns_off_ends_the_roll_at_rest() -> Result<()> {
    let (mut harness, clock) = clocked("12", true)?;
    set(&mut harness, "34");
    harness.render()?;
    after(&mut harness, &clock, 50)?;
    assert_ne!(screen(&harness), rows_of("34"));
    harness.canopy.set_motion_live(false);
    harness.render()?;
    assert_eq!(screen(&harness), rows_of("34"));
    assert!(!rolls(&mut harness));
    Ok(())
}

#[test]
fn without_motion_text_changes_at_once() -> Result<()> {
    let (mut harness, _clock) = clocked("12", false)?;
    set(&mut harness, "13");
    harness.render()?;
    assert_eq!(screen(&harness), rows_of("13"));
    assert!(!rolls(&mut harness));
    set(&mut harness, "13");
    assert!(!rolls(&mut harness), "the same text does not roll");
    Ok(())
}

#[test]
fn polls_drive_a_roll_to_its_end() -> Result<()> {
    let (mut harness, clock) = clocked("12", true)?;
    set(&mut harness, "99");
    let mut frames = 0;
    harness.wait_until(Duration::from_secs(10), |harness| {
        frames += 1;
        clock.advance(ROLL_FRAME)?;
        Ok(!rolls(harness))
    })?;
    harness.render()?;
    assert_eq!(screen(&harness), rows_of("99"));
    assert!(frames >= 6, "the roll took {frames} frames");
    Ok(())
}

/// A blank widget.
struct Blank;

impl Widget for Blank {
    fn render(&mut self, render: &mut Render<'_>, ctx: &dyn ViewContext) -> Result<()> {
        render.fill("", ctx.view().view_rect_local(), ' ')
    }
}

/// A root that scrolls a blank area and a text below it, so the blank
/// area can push the text out of view.
struct Below {
    /// The text to mount.
    text: Option<BigText>,
    /// The blank area.
    blank: Option<NodeId>,
}

impl Widget for Below {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let scroll: NodeId = ctx.add_child(ctx.node_id(), Scroll::vertical())?.into();
        let blank = ctx.add_child(scroll, Blank)?;
        self.blank = Some(blank.into());
        push(ctx, blank.into(), 0)?;
        let text = self.text.take().expect("text");
        ctx.add_child(scroll, text)?;
        Ok(())
    }
}

/// Sets the height of the blank area above the text.
fn push(ctx: &mut dyn Context, blank: NodeId, rows: u32) -> Result<()> {
    ctx.set_layout_override(
        blank,
        LayoutOverride::new().flex_horizontal(1).fixed_height(rows),
    )
}

/// Sets the height of the blank area of a harness and renders.
fn push_by(harness: &mut Harness, rows: u32) -> Result<()> {
    harness.with_root_widget_context(|below: &mut Below, ctx| {
        push(ctx, below.blank.expect("blank"), rows)
    })?;
    harness.render()
}

#[test]
fn text_in_a_scroll_keeps_its_size() -> Result<()> {
    let mut harness = Harness::builder(Below {
        text: Some(BigText::new("12")),
        blank: None,
    })
    .size(20, 3)
    .build()?;
    harness.render()?;
    assert_eq!(screen(&harness), rows_of("12"));
    Ok(())
}

#[test]
fn a_roll_out_of_view_ends_by_the_clock() -> Result<()> {
    let clock = Arc::new(ManualClock::new());
    let mut harness = Harness::builder(Below {
        text: Some(BigText::new("12")),
        blank: None,
    })
    .size(20, 3)
    .clock(Arc::clone(&clock))
    .motion(true)
    .build()?;
    harness.render()?;
    push_by(&mut harness, 3)?;
    harness.with_unique(|text: &mut BigText, _| {
        text.set_text("99");
        Ok(())
    })?;
    // Polls move the roll past its end, though no frame shows.
    let mut frames = 0;
    harness.wait_until(Duration::from_secs(10), |_| {
        frames += 1;
        clock.advance(ROLL_FRAME)?;
        Ok(frames >= 7)
    })?;
    // Then the polls stop.
    let mut turns = 0;
    let idle = harness.wait_until(Duration::from_millis(200), |_| {
        turns += 1;
        clock.advance(ROLL_FRAME)?;
        Ok(false)
    });
    assert!(idle.is_err());
    assert!(turns < 5, "{turns} turns after the end of the roll");
    // The text comes into view at rest.
    push_by(&mut harness, 0)?;
    assert_eq!(screen(&harness), rows_of("99"));
    assert!(!harness.with_unique(|text: &mut BigText, _| Ok(text.rolling()))?);
    Ok(())
}
