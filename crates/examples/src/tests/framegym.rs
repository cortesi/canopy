use canopy::{error::Result, geom, layout::Edges, prelude::*, testing::harness::Harness};

use super::{Mount, root_harness};
use crate::framegym::{FrameGym, FrameSlot, PatternSlot, binding_setup};

struct ViewMetrics {
    outer: geom::RectI32,
    content: geom::RectI32,
    canvas: geom::Size,
}

fn metrics(ctx: &dyn ViewContext) -> ViewMetrics {
    let view = ctx.view();
    ViewMetrics {
        outer: view.outer,
        content: view.content,
        canvas: view.canvas,
    }
}

fn framegym_harness() -> Result<Harness> {
    root_harness(FrameGym::new(), binding_setup, Size::new(20, 20), Mount::Replace)
}

fn frame_views(harness: &mut Harness) -> Result<(ViewMetrics, ViewMetrics, Layout)> {
    harness.with_root_context(|_root: &mut FrameGym, ctx| {
        ctx.with_typed_slot::<FrameSlot, _>(|_frame, frame_ctx| {
            let frame_view = metrics(frame_ctx);
            let frame_layout = frame_ctx.layout();
            let pattern_view = frame_ctx
                .with_typed_slot::<PatternSlot, _>(|_pattern, pattern_ctx| {
                    Ok(metrics(pattern_ctx))
                })?;
            Ok((frame_view, pattern_view, frame_layout))
        })
    })
}

fn pattern_scroll(harness: &mut Harness) -> Result<geom::Point> {
    harness.with_root_context(|_root: &mut FrameGym, ctx| {
        ctx.with_typed_slot::<FrameSlot, _>(|_frame, frame_ctx| {
            frame_ctx.with_typed_slot::<PatternSlot, _>(|_pattern, pattern_ctx| {
                Ok(pattern_ctx.view().scroll)
            })
        })
    })
}

#[test]
fn test_framegym_basic() -> Result<()> {
    let mut harness = framegym_harness()?;

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
    let mut harness = framegym_harness()?;

    let initial_scroll = pattern_scroll(&mut harness)?.y;
    harness.key('j')?;
    let updated_scroll = pattern_scroll(&mut harness)?.y;
    assert!(updated_scroll > initial_scroll);

    Ok(())
}

#[test]
fn framegym_scroll_commands_update_horizontal_scroll() -> Result<()> {
    let mut harness = framegym_harness()?;
    harness.render()?;

    let initial_scroll = pattern_scroll(&mut harness)?.x;
    harness.script(r#"test_pattern.scroll("Right")"#)?;
    let updated_scroll = pattern_scroll(&mut harness)?.x;
    assert!(updated_scroll > initial_scroll);

    Ok(())
}
