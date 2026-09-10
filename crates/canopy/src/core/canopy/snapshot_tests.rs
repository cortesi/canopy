//! Publication ownership, geometry flags, and phase boundaries.

use std::{cell::Cell, rc::Rc, sync::Arc};

use super::Canopy;
use crate::{
    FrameId, Invalidation, ViewContext, Widget, WidgetSemantics,
    core::snapshot,
    error::{Error, Result},
    geom::{RectI32, Size},
    layout::{Display, Layout},
};

struct Observed {
    /// Exposed value independent of live node lifetime.
    value: String,
    /// Counts actual semantic hooks, not snapshot reads.
    calls: Rc<Cell<u32>>,
    /// Injects a fallible observation after rendering.
    fail: Rc<Cell<bool>>,
}

impl Widget for Observed {
    fn semantics(&self, _view: &dyn ViewContext) -> Result<WidgetSemantics> {
        self.calls.set(self.calls.get() + 1);
        if self.fail.get() {
            return Err(Error::Invalid("semantic capture failed".into()));
        }
        Ok(WidgetSemantics {
            value: Some(self.value.clone()),
            ..WidgetSemantics::default()
        })
    }
}

struct Leaf;
impl Widget for Leaf {}

/// Make an application with explicit initial preparation still pending.
fn app() -> Result<Canopy> {
    let mut app = Canopy::new();
    app.set_root_size(Size::new(8, 4))?;
    app.finalize_api()?;
    Ok(app)
}

#[test]
fn immutable_reads_keep_old_nodes_values_and_frame_ids() -> Result<()> {
    let mut app = app()?;
    assert!(app.snapshot().is_none());
    let calls = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(false));
    let node = app.core.create_detached(Observed {
        value: "old".into(),
        calls: calls.clone(),
        fail,
    })?;
    app.flush()?;
    let old = app.snapshot().unwrap();
    let count = calls.get();
    for _ in 0..3 {
        assert!(Arc::ptr_eq(&old, &app.snapshot().unwrap()));
    }
    app.flush()?;
    assert!(Arc::ptr_eq(&old, &app.snapshot().unwrap()));
    assert_eq!(calls.get(), count);
    app.core.remove_subtree(node)?;
    app.flush()?;
    let new = app.snapshot().unwrap();
    assert!(new.frame_id.0 > old.frame_id.0);
    assert!(!new.nodes.iter().any(|entry| entry.id == node));
    assert_eq!(
        old.nodes
            .iter()
            .find(|entry| entry.id == node)
            .unwrap()
            .semantics
            .value
            .as_deref(),
        Some("old")
    );
    assert_eq!(old.cells.len(), 32);
    Ok(())
}

#[test]
fn failed_capture_preserves_publication_and_pending_changes() -> Result<()> {
    let mut app = app()?;
    let fail = Rc::new(Cell::new(false));
    app.core.create_detached(Observed {
        value: "value".into(),
        calls: Rc::new(Cell::new(0)),
        fail: fail.clone(),
    })?;
    app.flush()?;
    let old = app.snapshot().unwrap();
    let text = app.buf().unwrap().screen_text();
    fail.set(true);
    app.core.invalidate(Invalidation::Semantics);
    assert!(app.flush().is_err());
    assert!(Arc::ptr_eq(&old, &app.snapshot().unwrap()));
    assert_eq!(app.buf().unwrap().screen_text(), text);
    assert_eq!(FrameId(app.driver.publication.generation()), old.frame_id);
    assert!(app.core.changes.is_pending());
    fail.set(false);
    app.flush()?;
    assert_eq!(app.snapshot().unwrap().frame_id.0, old.frame_id.0 + 1);
    Ok(())
}

#[test]
fn flush_rejects_an_active_empty_widget_slot_before_preparation() -> Result<()> {
    let mut app = app()?;
    let root = app.core.root;
    let slot = app.core.nodes[root].widget.clone();
    let widget = slot.borrow_mut().take();
    app.core.callback_depth = 1;
    let result = app.flush();
    app.core.callback_depth = 0;
    *slot.borrow_mut() = widget;
    assert!(matches!(
        result,
        Err(Error::InvalidPhase { operation: "flush" })
    ));
    assert!(app.snapshot().is_none());
    app.flush()?;
    Ok(())
}

#[test]
fn capture_distinguishes_attachment_display_and_accumulated_clipping() -> Result<()> {
    let mut app = app()?;
    let root = app.core.root;
    let parent = app.core.create_detached(Leaf)?;
    let child = app.core.create_detached(Leaf)?;
    let detached = app.core.create_detached(Leaf)?;
    app.core.attach(root, parent)?;
    app.core.attach(parent, child)?;
    app.flush()?;
    // Explicit cached geometry isolates the observation contract from layout.
    app.core.nodes[parent].view.outer = RectI32::new(0, 0, 2, 2);
    app.core.nodes[parent].view.content = RectI32::new(0, 0, 2, 2);
    app.core.nodes[child].view.outer = RectI32::new(4, 0, 2, 2);
    let capture = |app: &Canopy| snapshot::capture(&app.core, FrameId(10), app.buf().unwrap());
    let snapshot = capture(&app)?;
    let child_entry = snapshot.nodes.iter().find(|node| node.id == child).unwrap();
    assert!(child_entry.attached && child_entry.displayed);
    assert!(!child_entry.intersects_viewport);
    assert_eq!(
        child_entry.view.map(|view| view.outer),
        Some(RectI32::new(4, 0, 2, 2))
    );
    let detached_entry = snapshot
        .nodes
        .iter()
        .find(|node| node.id == detached)
        .unwrap();
    assert!(!detached_entry.attached && !detached_entry.displayed);
    assert!(detached_entry.view.is_none());
    app.core.nodes[parent].hidden = true;
    let hidden = capture(&app)?;
    let child_entry = hidden.nodes.iter().find(|node| node.id == child).unwrap();
    assert!(child_entry.attached && !child_entry.displayed);
    assert!(child_entry.view.is_none());
    app.core.nodes[parent].hidden = false;
    app.core.nodes[parent].layout = Layout {
        display: Display::None,
        ..Layout::default()
    };
    let suppressed = capture(&app)?;
    assert!(
        !suppressed
            .nodes
            .iter()
            .find(|node| node.id == child)
            .unwrap()
            .displayed
    );
    app.core.nodes[parent].layout.display = Display::Block;
    app.core.nodes[child].view.outer = RectI32::new(-1, 0, 2, 2);
    assert!(
        capture(&app)?
            .nodes
            .iter()
            .find(|node| node.id == child)
            .unwrap()
            .intersects_viewport
    );
    app.core.nodes[child].view.outer = RectI32::new(-3, 0, 2, 2);
    assert!(
        !capture(&app)?
            .nodes
            .iter()
            .find(|node| node.id == child)
            .unwrap()
            .intersects_viewport
    );
    Ok(())
}
