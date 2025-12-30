use std::any::Any;

use super::{buf::BufTest, render::NopBackend};
use crate::{
    Canopy, Context, Loader, NodeId, ViewContext,
    core::{
        context::{CoreContext, CoreViewContext},
        termbuf::TermBuf,
    },
    error::Result,
    event::{key, mouse},
    geom::Expanse,
    widget::Widget,
};

/// A simple harness that holds a [`Canopy`], a [`NopBackend`] backend and a
/// root node ID. Tests drive the UI by sending key events and triggering renders
/// and can then inspect the render buffer.
pub struct Harness {
    /// The Canopy instance that manages the node tree and rendering.
    pub canopy: Canopy,
    /// The backend used for rendering. In tests, this is a no-op backend.
    pub backend: NopBackend,
    /// The root node of the UI under test.
    pub root: NodeId,
}

/// Builder for creating a test harness with a fluent API.
pub struct HarnessBuilder<W> {
    /// Root widget under test.
    root: W,
    /// View size for the harness.
    size: Expanse,
}

impl<W: Widget + Loader + 'static> HarnessBuilder<W> {
    /// Create a new harness builder with the given root widget.
    fn new(root: W) -> Self {
        Self {
            root,
            size: Expanse::new(100, 100),
        }
    }

    /// Set the size of the harness view.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Expanse::new(width, height);
        self
    }

    /// Build the harness with the configured settings.
    pub fn build(self) -> Result<Harness> {
        let render = NopBackend::new();
        let mut canopy = Canopy::new();

        <W as Loader>::load(&mut canopy);
        canopy.core.set_widget(canopy.core.root, self.root);
        canopy.core.with_layout_of(canopy.core.root, |layout| {
            *layout = (*layout).flex_horizontal(1).flex_vertical(1);
        })?;
        canopy.set_root_size(self.size)?;

        Ok(Harness {
            root: canopy.core.root,
            canopy,
            backend: render,
        })
    }
}

impl Harness {
    /// Create a harness builder for constructing a test harness with a fluent API.
    pub fn builder<W: Widget + Loader + 'static>(root: W) -> HarnessBuilder<W> {
        HarnessBuilder::new(root)
    }

    /// Create a harness using `size` for the root layout.
    pub fn with_size<W: Widget + Loader + 'static>(root: W, size: Expanse) -> Result<Self> {
        let render = NopBackend::new();
        let mut canopy = Canopy::new();
        <W as Loader>::load(&mut canopy);
        canopy.core.set_widget(canopy.core.root, root);
        canopy.set_root_size(size)?;
        Ok(Self {
            root: canopy.core.root,
            canopy,
            backend: render,
        })
    }

    /// Create a harness with a default root size of 100x100.
    pub fn new<W: Widget + Loader + 'static>(root: W) -> Result<Self> {
        Self::with_size(root, Expanse::new(100, 100))
    }

    /// Access the current render buffer. Panics if a render has not yet been performed.
    pub fn buf(&self) -> &TermBuf {
        self.canopy.buf().expect("render buffer not initialized")
    }

    /// Send a key event and render.
    pub fn key<T>(&mut self, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.canopy.key(k)?;
        self.canopy.render(&mut self.backend)
    }

    /// Send a mouse event and render.
    pub fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        self.canopy.mouse(m)?;
        self.canopy.render(&mut self.backend)
    }

    /// Send a sequence of key events and render after each.
    pub fn keys<I, K>(&mut self, keys: I) -> Result<()>
    where
        I: IntoIterator<Item = K>,
        K: Into<key::Key>,
    {
        for key in keys {
            self.key(key)?;
        }
        Ok(())
    }

    /// Type a string as a sequence of key events.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        self.keys(text.chars())
    }

    /// Render the root node into the harness backend.
    pub fn render(&mut self) -> Result<()> {
        self.canopy.render(&mut self.backend)
    }

    /// Render and return a snapshot of the buffer contents.
    pub fn render_snapshot(&mut self) -> Result<String> {
        self.render()?;
        Ok(self.tbuf().snapshot())
    }

    /// Execute a script on the app under test.
    pub fn script(&mut self, script: &str) -> Result<()> {
        let script_id = self.canopy.script_host.compile(script)?;
        self.canopy.run_script(self.root, script_id)?;
        self.canopy.render(&mut self.backend)
    }

    /// Execute a closure with mutable access to a widget by node id.
    pub fn with_widget<W, R>(&mut self, node_id: NodeId, f: impl FnOnce(&mut W) -> R) -> R
    where
        W: Widget + 'static,
    {
        self.canopy.core.with_widget_mut(node_id, |widget, _| {
            let any = widget as &mut dyn Any;
            let widget = any.downcast_mut::<W>().expect("widget type mismatch");
            f(widget)
        })
    }

    /// Execute a closure with mutable access to the root widget.
    pub fn with_root_widget<W, R>(&mut self, f: impl FnOnce(&mut W) -> R) -> R
    where
        W: Widget + 'static,
    {
        let root = self.root;
        self.with_widget(root, f)
    }

    /// Execute a closure with mutable access to the root widget and a context.
    pub fn with_root_context<W, R>(
        &mut self,
        mut f: impl FnMut(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let root = self.root;
        self.canopy.core.with_widget_mut(root, |widget, core| {
            let mut ctx = CoreContext::new(core, root);
            let any = widget as &mut dyn Any;
            let widget = any
                .downcast_mut::<W>()
                .expect("with_root_context: widget type mismatch");
            f(widget, &mut ctx)
        })
    }

    /// Get a BufTest instance that references the current buffer.
    pub fn tbuf(&self) -> BufTest<'_> {
        BufTest::new(self.buf())
    }

    /// Find the first node whose path matches the filter, relative to the root.
    pub fn find_node(&self, path_filter: &str) -> Option<NodeId> {
        let ctx = CoreViewContext::new(&self.canopy.core, self.root);
        ctx.find_node(path_filter)
    }

    /// Find all nodes whose paths match the filter, relative to the root.
    pub fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {
        let ctx = CoreViewContext::new(&self.canopy.core, self.root);
        ctx.find_nodes(path_filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ViewContext, derive_commands, error::Result, geom::Line, layout::Layout, render::Render,
        state::NodeName, widget::Widget,
    };

    struct TestNode;

    #[derive_commands]
    impl TestNode {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for TestNode {
        fn layout(&self) -> Layout {
            Layout::fill()
        }

        fn render(&mut self, r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            r.text("base", Line::new(0, 0, 5), "test")?;
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("test_node")
        }
    }

    impl Loader for TestNode {}

    #[test]
    fn test_harness_dump() {
        let mut h = Harness::builder(TestNode::new())
            .size(10, 3)
            .build()
            .unwrap();
        h.render().unwrap();

        h.tbuf().dump();
        assert!(h.tbuf().contains_text("test"));
    }

    #[test]
    fn test_harness_builder() {
        let mut h = Harness::builder(TestNode::new())
            .size(20, 5)
            .build()
            .unwrap();

        h.render().unwrap();
        assert!(h.tbuf().contains_text("test"));
    }

    #[test]
    fn test_harness_with_size() {
        let mut h = Harness::with_size(TestNode::new(), Expanse::new(15, 4)).unwrap();
        h.render().unwrap();
        assert!(h.tbuf().contains_text("test"));
    }
}
