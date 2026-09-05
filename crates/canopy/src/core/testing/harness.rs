use super::buf::BufTest;
use crate::{
    Canopy, Context, Loader, NodeId,
    core::termbuf::TermBuf,
    error::Result,
    event::{Event, key, mouse},
    geom::Size,
    layout::Sizing,
    render::NopBackend,
    widget::Widget,
};

/// A simple harness that holds a [`Canopy`], a [`NopBackend`] backend and a
/// root node ID. Tests drive the UI by sending key events and triggering
/// renders and can then inspect the render buffer.
pub struct Harness {
    /// The Canopy instance that manages the node tree and rendering.
    pub canopy: Canopy,
    /// The backend used for rendering. In tests, this is a no-op backend.
    backend: NopBackend,
    /// The root node of the UI under test.
    pub root: NodeId,
}

/// Builder for creating a test harness with a fluent API.
pub struct HarnessBuilder<W> {
    /// Root widget under test.
    root: W,
    /// View size for the harness.
    size: Size,
}

impl<W: Widget + Loader + 'static> HarnessBuilder<W> {
    /// Create a new harness builder with the given root widget.
    fn new(root: W) -> Self {
        Self {
            root,
            size: Size::new(100, 100),
        }
    }

    /// Set the size of the harness view.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    /// Build the harness with the configured settings.
    pub fn build(self) -> Result<Harness> {
        let render = NopBackend::new();
        let mut canopy = Canopy::new();

        <W as Loader>::load(&mut canopy)?;
        canopy.finalize_api()?;
        canopy.replace_root(self.root)?;
        canopy.core.with_layout_of(canopy.core.root, |layout| {
            *layout = layout.width(Sizing::Flex(1)).height(Sizing::Flex(1));
        })?;
        canopy.set_root_size(self.size)?;
        canopy.turn(crate::Work::Prepare)?;

        Ok(Harness {
            root: canopy.core.root,
            canopy,
            backend: render,
        })
    }
}

impl Harness {
    /// Wrap an already configured Canopy application in a test harness.
    pub fn from_canopy(mut canopy: Canopy, size: Size) -> Result<Self> {
        canopy.set_root_size(size)?;
        canopy.turn(crate::Work::Prepare)?;
        let root = canopy.root_id();
        Ok(Self {
            canopy,
            backend: NopBackend::new(),
            root,
        })
    }

    /// Create a harness builder for constructing a test harness with a fluent
    /// API.
    pub fn builder<W: Widget + Loader + 'static>(root: W) -> HarnessBuilder<W> {
        HarnessBuilder::new(root)
    }

    /// Create a harness with the builder's default root size.
    pub fn new<W: Widget + Loader + 'static>(root: W) -> Result<Self> {
        Self::builder(root).build()
    }

    /// Access the current render buffer. Panics if a render has not yet been
    /// performed.
    pub fn buf(&self) -> &TermBuf {
        self.canopy.buf().expect("render buffer not initialized")
    }

    /// Send a key event and render.
    pub fn key<T>(&mut self, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.canopy
            .turn(crate::Work::Input(Event::Key(k.into())))
            .map(|_| ())
    }

    /// Send a mouse event and render.
    pub fn mouse(&mut self, m: mouse::MouseEvent) -> Result<()> {
        self.canopy
            .turn(crate::Work::Input(Event::Mouse(m)))
            .map(|_| ())
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

    /// Execute a script on the app under test.
    pub fn script(&mut self, script: &str) -> Result<()> {
        self.canopy.eval_script(script)
    }

    /// Execute a closure with mutable access to a widget by node id.
    pub fn with_widget<W, R>(
        &mut self,
        node_id: impl Into<NodeId>,
        f: impl FnOnce(&mut W) -> R,
    ) -> R
    where
        W: Widget + 'static,
    {
        let node_id = node_id.into();
        self.canopy
            .with_context(node_id, |ctx| {
                ctx.with_node(node_id, |widget, _| Ok(f(widget)))
            })
            .expect("with_widget failed")
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
        f: impl FnOnce(&mut W, &mut dyn Context) -> Result<R>,
    ) -> Result<R>
    where
        W: Widget + 'static,
    {
        let root = self.root;
        self.canopy.with_root_context(|ctx| ctx.with_node(root, f))
    }

    /// Get a BufTest instance that references the current buffer.
    pub fn tbuf(&self) -> BufTest<'_> {
        BufTest::new(self.buf())
    }

    /// Find all nodes whose paths match the filter, relative to the root.
    pub fn find_nodes(&self, path_filter: &str) -> Vec<NodeId> {
        self.canopy
            .with_root_view(|context| context.find_nodes(path_filter))
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
    fn test_harness_builder() {
        let mut h = Harness::builder(TestNode::new())
            .size(20, 5)
            .build()
            .unwrap();

        h.render().unwrap();
        assert!(h.tbuf().contains_text("test"));
    }
}
