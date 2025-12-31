use canopy::{
    error::Result,
    event::{key, mouse},
    geom,
    layout::Edges,
    testing::harness::Harness,
};

use crate::framegym::FrameGym;

#[test]
fn test_framegym_basic() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let frame_id = harness.find_node("*/frame").expect("missing frame");
    let pattern_id = harness
        .find_node("*/frame/test_pattern")
        .expect("missing pattern");
    let core = &harness.canopy.core;
    let frame_view = core.node(frame_id).expect("missing frame").view();
    let pattern_view = core.node(pattern_id).expect("missing pattern").view();
    let frame_layout = core.node(frame_id).expect("missing frame").layout();

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
fn framegym_scrollbar_drag_updates_scroll() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let frame_id = harness.find_node("*/frame").expect("missing frame");
    let pattern_id = harness
        .find_node("*/frame/test_pattern")
        .expect("missing pattern");
    let core = &harness.canopy.core;
    let frame_view = core.node(frame_id).expect("missing frame").view();
    let pattern_view = core.node(pattern_id).expect("missing pattern").view();

    let outer_local = frame_view.outer_rect_local();
    let frame_geom = geom::Frame::new(outer_local, 1);
    let active = pattern_view
        .vactive(frame_geom.right)?
        .expect("missing active scrollbar")
        .1;
    let start_local = geom::Point {
        x: active.tl.x,
        y: active.tl.y.saturating_add(active.h / 2),
    };
    let start_screen = geom::Point {
        x: (frame_view.outer.tl.x + start_local.x as i32).max(0) as u32,
        y: (frame_view.outer.tl.y + start_local.y as i32).max(0) as u32,
    };

    let initial_scroll = pattern_view.tl.y;
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: start_screen,
    })?;
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Drag,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: geom::Point {
            x: start_screen.x,
            y: start_screen.y.saturating_add(3),
        },
    })?;

    let updated_scroll = harness
        .canopy
        .core
        .node(pattern_id)
        .expect("missing pattern")
        .view()
        .tl
        .y;
    assert!(updated_scroll > initial_scroll);

    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Up,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: geom::Point {
            x: start_screen.x,
            y: start_screen.y.saturating_add(3),
        },
    })?;

    Ok(())
}

#[test]
fn framegym_horizontal_scrollbar_drag_updates_scroll() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let frame_id = harness.find_node("*/frame").expect("missing frame");
    let pattern_id = harness
        .find_node("*/frame/test_pattern")
        .expect("missing pattern");
    let core = &harness.canopy.core;
    let frame_view = core.node(frame_id).expect("missing frame").view();
    let pattern_view = core.node(pattern_id).expect("missing pattern").view();

    let outer_local = frame_view.outer_rect_local();
    let frame_geom = geom::Frame::new(outer_local, 1);
    let active = pattern_view
        .hactive(frame_geom.bottom)?
        .expect("missing active scrollbar")
        .1;
    let start_local = geom::Point {
        x: active.tl.x.saturating_add(active.w / 2),
        y: active.tl.y,
    };
    let start_screen = geom::Point {
        x: (frame_view.outer.tl.x + start_local.x as i32).max(0) as u32,
        y: (frame_view.outer.tl.y + start_local.y as i32).max(0) as u32,
    };

    let initial_scroll = pattern_view.tl.x;
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Down,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: start_screen,
    })?;
    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Drag,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: geom::Point {
            x: start_screen.x.saturating_add(3),
            y: start_screen.y,
        },
    })?;

    let updated_scroll = harness
        .canopy
        .core
        .node(pattern_id)
        .expect("missing pattern")
        .view()
        .tl
        .x;
    assert!(updated_scroll > initial_scroll);

    harness.mouse(mouse::MouseEvent {
        action: mouse::Action::Up,
        button: mouse::Button::Left,
        modifiers: key::Empty,
        location: geom::Point {
            x: start_screen.x.saturating_add(3),
            y: start_screen.y,
        },
    })?;

    Ok(())
}
