//! Motion at emission and soft cursors, driven by a manual clock.

use std::{sync::Arc, time::Duration};

use super::{Canopy, CanopyBuilder, MotionSettings};
use crate::{
    NodeId, ViewContext, Widget,
    commands::ArgValue,
    core::cursor::{self, CursorMotion, CursorRequest, CursorShape},
    error::Result,
    geom::{Line, Point, Size},
    input::{Event, key},
    layout::Layout,
    render::Render,
    runtime::TurnInput,
    style::{
        Animation, Color, Drift, GradientSpec, GradientStop, Paint, Pause, ResolvedStyle, themes,
    },
    testing::{ManualClock, backend::TestRender},
    text,
};

/// Paints text in a style path, and declares a cursor.
struct Spot {
    /// Style path of the text.
    style: &'static str,
    /// Text painted from the left edge, in cells of its own width.
    text: &'static str,
    /// Local column and role of the cursor.
    cursor: Option<(u32, &'static str)>,
    /// Layout of the node.
    layout: Layout,
}

impl Spot {
    /// A spot that fills its parent.
    fn new(style: &'static str, text: &'static str) -> Self {
        Self {
            style,
            text,
            cursor: None,
            layout: Layout::fill(),
        }
    }

    /// Declare a cursor at a local column.
    fn with_cursor(mut self, x: u32, role: &'static str) -> Self {
        self.cursor = Some((x, role));
        self
    }

    /// Replace the layout.
    fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

impl Widget for Spot {
    fn layout(&self) -> Layout {
        self.layout
    }

    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let rect = ctx.view().outer_rect_local();
        render.fill("", rect, ' ')?;
        let width = text::width(self.text);
        render.text(self.style, Line::new(0, 0, width), self.text)?;
        if let Some((x, role)) = self.cursor {
            render.cursor(Point { x, y: 0 }, CursorRequest::new(role));
        }
        Ok(())
    }
}

/// Milliseconds.
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// A red and blue blink of 100 ms each.
fn blink() -> Paint {
    Paint::animated(Animation::blink(Color::Red, Color::Blue, ms(100), ms(100)))
}

/// Build a live app of the given width with a root spot and a manual clock.
fn live(root: Spot, width: u32, style: Option<Paint>) -> Result<(Canopy, Arc<ManualClock>)> {
    let mut canopy = CanopyBuilder::new().build()?;
    let clock = Arc::new(ManualClock::new());
    canopy.set_clock_for_testing(Arc::clone(&clock))?;
    canopy.replace_root(root)?;
    canopy.set_screen_size(Size::new(width, 1))?;
    if let Some(style) = style {
        canopy.style_mut().rules().fg("moving", style).apply();
    }
    canopy.set_motion_live(true);
    Ok((canopy, clock))
}

/// Return the style of one cell of the last emitted buffer.
fn emitted(canopy: &Canopy, x: u32) -> ResolvedStyle {
    canopy
        .frame
        .emitted_buf
        .as_ref()
        .and_then(|buf| buf.get(Point { x, y: 0 }))
        .expect("emitted cell")
        .style
}

/// Return the background of the theme's ground.
fn ground(canopy: &Canopy) -> Color {
    canopy
        .style
        .resolve("")
        .bg
        .solid_color()
        .expect("solid ground")
}

/// Return the style of one cell of the published buffer.
fn published(canopy: &Canopy, x: u32) -> ResolvedStyle {
    canopy
        .published_buf()
        .and_then(|buf| buf.get(Point { x, y: 0 }))
        .expect("published cell")
        .style
}

#[test]
fn moving_cells_emit_without_a_frame_and_snapshots_stay_at_rest() -> Result<()> {
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 3, Some(blink()))?;
    let mut backend = TestRender::new();
    let t0 = clock.now();
    assert!(canopy.turn(TurnInput::Prepare)?.frame.is_some());
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Red);
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(100)));

    clock.advance(ms(100))?;
    let outcome = canopy.turn(TurnInput::Wake)?;
    assert!(outcome.frame.is_none());
    assert!(outcome.motion);
    canopy.emit_frame(&mut backend)?;
    // Only the moving cell is written.
    assert_eq!(backend.writes.len(), 1);
    assert_eq!(backend.writes[0].0, Point::ZERO);
    assert_eq!(backend.writes[0].1.fg, Color::Blue);
    assert_eq!(emitted(&canopy, 0).fg, Color::Blue);
    assert_eq!(published(&canopy, 0).fg, Color::Red, "the snapshot rests");
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(200)));

    // A wake before the next edge changes nothing.
    clock.advance(ms(50))?;
    assert!(!canopy.turn(TurnInput::Wake)?.motion);
    Ok(())
}

#[test]
fn a_diff_and_a_full_repaint_emit_the_same_screen() -> Result<()> {
    let (mut canopy, clock) = live(Spot::new("moving", "xy"), 4, Some(blink()))?;
    let mut backend = TestRender::new();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    let diffed = canopy.frame.emitted_buf.clone().expect("emitted");
    canopy.frame.emitted_buf = None;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(canopy.frame.emitted_buf, Some(diffed));
    Ok(())
}

#[test]
fn disabled_or_headless_motion_stays_at_rest() -> Result<()> {
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 2, Some(blink()))?;
    canopy.set_motion(MotionSettings {
        enabled: false,
        ..MotionSettings::default()
    });
    let mut backend = TestRender::new();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(canopy.next_deadline(), None);
    clock.advance(ms(100))?;
    assert!(!canopy.turn(TurnInput::Wake)?.motion);

    canopy.set_motion(MotionSettings::default());
    canopy.set_motion_live(false);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Red);
    assert_eq!(canopy.next_deadline(), None);
    Ok(())
}

#[test]
fn continuous_motion_samples_at_the_frame_rate_limit() -> Result<()> {
    let pulse = Paint::animated(Animation::pulse(Color::Red, Color::Blue, ms(1000)));
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 2, Some(pulse))?;
    canopy.set_motion(MotionSettings {
        max_fps: 10,
        ..MotionSettings::default()
    });
    let mut backend = TestRender::new();
    let t0 = clock.now();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(100)));
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_ne!(emitted(&canopy, 0).fg, Color::Red);
    Ok(())
}

#[test]
fn repeating_motion_rests_after_the_idle_pause() -> Result<()> {
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 2, Some(blink()))?;
    canopy.set_motion(MotionSettings {
        idle_pause: ms(250),
        ..MotionSettings::default()
    });
    let mut backend = TestRender::new();
    let t0 = clock.now();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Blue);
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(250)));

    // The pause holds the loop at rest, and nothing wakes an idle app.
    clock.advance(ms(100))?;
    canopy.turn(TurnInput::Wake)?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Red);
    assert_eq!(canopy.next_deadline(), None);

    // Input resumes it.
    let key = Event::Key(key::KeyCode::Char('z').into());
    canopy.turn(TurnInput::Events(vec![key]))?;
    canopy.emit_frame(&mut backend)?;
    assert!(canopy.next_deadline().is_some());
    Ok(())
}

#[test]
fn a_busy_loop_moves_on_through_the_idle_pause() -> Result<()> {
    let busy = Paint::animated(
        Animation::blink(Color::Red, Color::Blue, ms(100), ms(100)).with_pause(Pause::Never),
    );
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 2, Some(busy))?;
    canopy.set_motion(MotionSettings {
        idle_pause: ms(250),
        ..MotionSettings::default()
    });
    let mut backend = TestRender::new();
    let t0 = clock.now();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    clock.advance(ms(300))?;
    canopy.turn(TurnInput::Wake)?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Blue);
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(400)));
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg, Color::Red);
    Ok(())
}

#[test]
fn a_fade_runs_once_from_the_first_frame_that_shows_it() -> Result<()> {
    let fade = Paint::animated(Animation::fade(Color::White, Color::Black, ms(100)));
    let (mut canopy, clock) = live(Spot::new("moving", "x"), 2, Some(fade))?;
    let mut backend = TestRender::new();
    let t0 = clock.now();
    canopy.turn(TurnInput::Prepare)?;
    assert_eq!(
        published(&canopy, 0).fg.rgb(),
        (0, 0, 0),
        "the snapshot shows the end"
    );
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg.rgb(), (255, 255, 255));
    clock.advance(ms(100))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 0).fg.rgb(), (0, 0, 0));
    // Only the idle pause remains, and a finished fade records no motion.
    assert_eq!(canopy.next_deadline(), Some(t0 + ms(10_000)));
    canopy.style_mut();
    canopy.turn(TurnInput::Prepare)?;
    assert!(!canopy.published_buf().expect("published").has_motion());
    Ok(())
}

/// Build a row of three one-cell spots under a root, each declaring a
/// cursor, with the middle one focused.
fn cursor_row() -> Result<(Canopy, Arc<ManualClock>, [NodeId; 3])> {
    let root = Spot::new("", "").with_layout(Layout::row().flex_horizontal(1).flex_vertical(1));
    let (mut canopy, clock) = live(root, 3, None)?;
    let cell = |role| {
        Spot::new("", "a")
            .with_cursor(0, role)
            .with_layout(Layout::row().fixed_width(1).flex_vertical(1))
    };
    let nodes = [
        canopy.core.create_detached(cell(cursor::VI_VISUAL))?,
        canopy.core.create_detached(cell(cursor::VI_INSERT))?,
        canopy.core.create_detached(cell(cursor::INACTIVE))?,
    ];
    let root = canopy.core.root;
    canopy.core.set_children(root, nodes.to_vec())?;
    canopy.core.set_focus(nodes[1])?;
    Ok((canopy, clock, nodes))
}

#[test]
fn the_primary_cursor_leads_and_only_it_moves() -> Result<()> {
    let (mut canopy, _clock, nodes) = cursor_row()?;
    canopy.turn(TurnInput::Prepare)?;
    let snapshot = canopy.snapshot().expect("published");
    let cursors: Vec<_> = snapshot
        .cursors
        .iter()
        .map(|c| {
            (
                c.node,
                c.location.x,
                c.role.as_str(),
                c.primary,
                c.look.motion,
            )
        })
        .collect();
    let palette = themes::default_dark();
    assert_eq!(
        cursors,
        [
            (nodes[1], 1, cursor::VI_INSERT, true, CursorMotion::BLINK),
            (nodes[0], 0, cursor::VI_VISUAL, false, CursorMotion::Steady),
            (nodes[2], 2, cursor::INACTIVE, false, CursorMotion::Steady),
        ]
    );
    assert_eq!(snapshot.cursors[0].look.color, palette.green);
    assert_eq!(published(&canopy, 1).bg, palette.green);
    assert_eq!(published(&canopy, 0).bg, palette.magenta);
    Ok(())
}

#[test]
fn a_blinking_cursor_hides_then_restarts_on_input() -> Result<()> {
    let (mut canopy, clock, _) = cursor_row()?;
    let palette = themes::default_dark();
    let mut backend = TestRender::new();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(backend.parked, Some(Point { x: 1, y: 0 }));
    assert_eq!(emitted(&canopy, 1).bg, palette.green);

    clock.advance(cursor::BLINK_ON)?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_eq!(
        emitted(&canopy, 1).bg,
        ground(&canopy),
        "the cell below shows"
    );
    assert_eq!(
        emitted(&canopy, 0).bg,
        palette.magenta,
        "secondary cursors stay"
    );

    let key = Event::Key(key::KeyCode::Char('z').into());
    canopy.turn(TurnInput::Events(vec![key]))?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(
        emitted(&canopy, 1).bg,
        palette.green,
        "input shows the cursor"
    );
    Ok(())
}

#[test]
fn an_unfocused_terminal_dims_the_primary_cursor_and_pauses() -> Result<()> {
    let (mut canopy, _clock, _) = cursor_row()?;
    let palette = themes::default_dark();
    let mut backend = TestRender::new();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    let outcome = canopy.turn(TurnInput::Events(vec![Event::FocusLost]))?;
    if outcome.frame.is_some() || outcome.motion {
        canopy.emit_frame(&mut backend)?;
    }
    let dimmed = emitted(&canopy, 1).bg;
    assert_ne!(dimmed, palette.green);
    assert_ne!(dimmed, ground(&canopy));
    assert_eq!(canopy.next_deadline(), None);

    canopy.turn(TurnInput::Events(vec![Event::FocusGained]))?;
    canopy.emit_frame(&mut backend)?;
    assert_eq!(emitted(&canopy, 1).bg, palette.green);
    Ok(())
}

#[test]
fn cursor_looks_follow_the_theme_and_keep_script_replacements() -> Result<()> {
    let (mut canopy, _clock, _) = cursor_row()?;
    canopy.eval_script(
        r##"canopy.set_cursor_look("cursor/vi/insert", { color = "#ff0000", motion = "pulse", shape = "underline" })"##,
    )?;
    canopy.set_theme(themes::dracula());
    let insert = canopy.cursor_looks().resolve(cursor::VI_INSERT);
    assert_eq!(insert.color, Color::Rgb { r: 255, g: 0, b: 0 });
    assert_eq!(insert.motion, CursorMotion::PULSE);
    assert_eq!(insert.shape, CursorShape::Underline);
    assert_eq!(
        canopy.cursor_looks().resolve(cursor::VI_NORMAL).color,
        themes::dracula().blue
    );
    let role = canopy.eval_script(
        "canopy.prepare()\nlocal snapshot = canopy.snapshot()\nreturn snapshot and snapshot.cursors[1].shape",
    )?;
    assert_eq!(role, ArgValue::String("underline".into()));
    assert!(
        canopy
            .eval_script(r##"canopy.set_cursor_look("cursor", { color = "red" })"##)
            .is_err()
    );
    Ok(())
}

#[test]
fn script_motion_settings_replace_only_the_given_fields() -> Result<()> {
    let (mut canopy, _clock, _) = cursor_row()?;
    canopy.eval_script("canopy.set_motion({ max_fps = 12, idle_ms = 500 })")?;
    assert_eq!(
        canopy.motion(),
        MotionSettings {
            enabled: true,
            max_fps: 12,
            idle_pause: ms(500),
        }
    );
    canopy.eval_script("canopy.set_motion({ enabled = false })")?;
    assert!(!canopy.motion().enabled);
    assert!(
        canopy
            .eval_script("canopy.set_motion({ max_fps = 0 })")
            .is_err()
    );
    assert!(
        canopy
            .eval_script("canopy.set_motion({ speed = 2 })")
            .is_err()
    );
    Ok(())
}

#[test]
fn a_drifting_gradient_moves_with_the_epoch() -> Result<()> {
    let gradient = Paint::gradient(
        GradientSpec::with_stops(
            0.0,
            vec![
                GradientStop::new(0.0, Color::Black),
                GradientStop::new(1.0, Color::White),
            ],
        )
        .with_drift(Drift::Slide(ms(1000))),
    );
    let (mut canopy, clock) = live(Spot::new("moving", "abcd"), 4, Some(gradient))?;
    let mut backend = TestRender::new();
    canopy.turn(TurnInput::Prepare)?;
    canopy.emit_frame(&mut backend)?;
    let before = emitted(&canopy, 0).fg;
    clock.advance(ms(500))?;
    assert!(canopy.turn(TurnInput::Wake)?.motion);
    canopy.emit_frame(&mut backend)?;
    assert_ne!(emitted(&canopy, 0).fg, before);
    assert_eq!(emitted(&canopy, 0).fg, published(&canopy, 2).fg);
    Ok(())
}
