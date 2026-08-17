//! Widgets and node builders shared by the world and layout-driver test modules.

use std::sync::{Arc, Mutex};

use crate::{
    NodeId,
    core::world::Core,
    error::{Error, Result},
    geom::Size,
    layout::{CanvasContext, Layout, MeasureConstraints, Measurement},
    widget::Widget,
};

/// Measurement hook installed on a [`TestWidget`].
pub(super) type MeasureFn = dyn Fn(MeasureConstraints) -> Measurement + Send + Sync;
/// Canvas hook installed on a [`TestWidget`].
pub(super) type CanvasFn = dyn Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync;

/// A widget whose measure and canvas behavior a test supplies, recording every measure call.
pub(super) struct TestWidget {
    /// Measurement hook.
    measure_fn: Arc<MeasureFn>,
    /// Canvas hook.
    canvas_fn: Arc<CanvasFn>,
}

impl TestWidget {
    /// Build a widget with a measure hook and the identity canvas.
    pub(super) fn new<F>(measure_fn: F) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
    where
        F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
    {
        Self::with_canvas(measure_fn, |view, _ctx| view)
    }

    /// Build a widget with both a measure and a canvas hook.
    pub(super) fn with_canvas<F, C>(
        measure_fn: F,
        canvas_fn: C,
    ) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
    where
        F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
        C: Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync + 'static,
    {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let measure_fn = Arc::new(move |c: MeasureConstraints| {
            calls_clone.lock().unwrap().push(c);
            measure_fn(c)
        });
        let canvas_fn = Arc::new(canvas_fn);
        (
            Self {
                measure_fn,
                canvas_fn,
            },
            calls,
        )
    }
}

impl Widget for TestWidget {
    fn measure(&self, c: MeasureConstraints) -> Measurement {
        (self.measure_fn)(c)
    }

    fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
        (self.canvas_fn)(view, ctx)
    }
}

/// A widget that reports one fixed layout.
pub(super) struct LayoutWidget(pub(super) Layout);

impl Widget for LayoutWidget {
    fn layout(&self) -> Layout {
        self.0
    }
}

/// Create a detached node that measures to a fixed size.
pub(super) fn fixed_leaf(core: &mut Core, width: u32, height: u32) -> Result<NodeId> {
    let (widget, _) = TestWidget::new(move |_c| Measurement::Fixed(Size::new(width, height)));
    core.create_detached(widget)
}

/// Create a detached node that wraps its children.
pub(super) fn wrap_node(core: &mut Core) -> Result<NodeId> {
    let (widget, _) = TestWidget::new(|_c| Measurement::Wrap);
    core.create_detached(widget)
}

/// Assert that a node operation error carries its operation, node, path, and source.
pub fn assert_error_context(error: &Error, operation: &str, node_id: NodeId, path: &str) {
    let Error::NodeOperation {
        operation: actual_operation,
        node,
        path: actual_path,
        source,
        ..
    } = error
    else {
        panic!("expected node operation error, got {error:?}");
    };
    assert_eq!(*actual_operation, operation);
    assert_eq!(*node, node_id);
    assert_eq!(actual_path, path);
    assert!(!source.to_string().is_empty());
}
