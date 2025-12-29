use canopy::{error::Result, layout::Edges, testing::harness::Harness};

use crate::framegym::FrameGym;

#[test]
fn test_framegym_basic() -> Result<()> {
    let mut harness = Harness::builder(FrameGym::new()).size(20, 20).build()?;
    harness.render()?;

    let frame_id = harness.find_node("*/frame").expect("missing frame");
    let pattern_id = harness
        .find_node("*/frame/test_pattern")
        .expect("missing pattern");
    let frame_view = harness.canopy.core.nodes[frame_id].view;
    let pattern_view = harness.canopy.core.nodes[pattern_id].view;
    let frame_layout = &harness.canopy.core.nodes[frame_id].layout;

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
