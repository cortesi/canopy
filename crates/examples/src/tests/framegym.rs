use canopy::{
    ViewContext,
    error::Result,
    geom,
    layout::{Edges, Layout},
    testing::harness::Harness,
    widgets::frame,
};

use crate::framegym::{FrameGym, KEY_FRAME, KEY_PATTERN, TestPattern};

struct ViewMetrics {
    outer: geom::RectI32,
    content: geom::RectI32,
    canvas: geom::Expanse,
}

fn metrics(ctx: &dyn ViewContext) -> ViewMetrics {
    let view = ctx.view();
    ViewMetrics {
        outer: view.outer,
        content: view.content,
        canvas: view.canvas,
    }
}

fn frame_views(harness: &mut Harness) -> Result<(ViewMetrics, ViewMetrics, Layout)> {
    harness.with_root_context(|_root: &mut FrameGym, ctx| {
        ctx.with_keyed::<frame::Frame, _>(KEY_FRAME, |_frame, frame_ctx| {
            let frame_view = metrics(frame_ctx);
            let frame_layout = frame_ctx.layout();
            let pattern_view = frame_ctx
                .with_keyed::<TestPattern, _>(KEY_PATTERN, |_pattern, pattern_ctx| {
                    Ok(metrics(pattern_ctx))
                })?;
            Ok((frame_view, pattern_view, frame_layout))
        })
    })
}

fn pattern_scroll(harness: &mut Harness) -> Result<geom::Point> {
    harness.with_root_context(|_root: &mut FrameGym, ctx| {
        ctx.with_keyed::<frame::Frame, _>(KEY_FRAME, |_frame, frame_ctx| {
            frame_ctx.with_keyed::<TestPattern, _>(KEY_PATTERN, |_pattern, pattern_ctx| {
                Ok(pattern_ctx.view().tl)
            })
        })
    })
}

#[test]
fn test_framegym_basic() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let (frame_view, pattern_view, frame_layout) = frame_views(&mut harness)?;

    assert_eq!(pattern_view.outer.tl.x, frame_view.content.tl.x);
    assert_eq!(pattern_view.outer.tl.y, frame_view.content.tl.y);
    assert_eq!(frame_view.canvas.w, frame_view.content.w);
    assert_eq!(frame_view.canvas.h, frame_view.content.h);
    assert_eq!(frame_layout.padding, Edges::all(1));
    assert_eq!(pattern_view.outer.w + 2, frame_view.outer.w);
    assert_eq!(pattern_view.outer.h + 2, frame_view.outer.h);

    let lines = harness.tbuf().lines();
    let last_col = lines[0].chars().count() - 1;
    assert_eq!(lines[0].chars().next(), Some('\u{256d}'));
    assert_eq!(lines[0].chars().nth(last_col), Some('\u{256e}'));
    assert_eq!(lines[19].chars().next(), Some('\u{2570}'));
    assert_eq!(lines[19].chars().nth(last_col), Some('\u{256f}'));

    for line in &lines[1..19] {
        assert_eq!(line.chars().next(), Some('\u{2502}'));
        let right = line.chars().nth(last_col);
        assert!(matches!(right, Some('\u{2502}' | '\u{2588}')));
    }
    Ok(())
}

#[test]
fn framegym_scroll_commands_update_vertical_scroll() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let initial_scroll = pattern_scroll(&mut harness)?.y;
    harness.script("test_pattern::scroll_down()")?;
    let updated_scroll = pattern_scroll(&mut harness)?.y;
    assert!(updated_scroll > initial_scroll);

    Ok(())
}

#[test]
fn framegym_scroll_commands_update_horizontal_scroll() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let initial_scroll = pattern_scroll(&mut harness)?.x;
    harness.script("test_pattern::scroll_right()")?;
    let updated_scroll = pattern_scroll(&mut harness)?.x;
    assert!(updated_scroll > initial_scroll);

    Ok(())
}
