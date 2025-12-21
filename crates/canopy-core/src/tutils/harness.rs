use super::{buf::BufTest, render::NopBackend};
use crate::{Canopy, Loader, Node, Result, TermBuf, event::key, geom::Expanse};

/// A simple harness that holds a [`Canopy`], a [`DummyBackend`] backend and a
/// root node. Tests drive the UI by sending key events and triggering renders
/// and can then inspect the render buffer.
pub struct Harness<N> {
    /// The Canopy instance that manages the node tree and rendering.
    pub canopy: Canopy,
    /// The backend used for rendering. In tests, this is a no-op backend.
    pub backend: NopBackend,
    /// The root node of the UI under test.
    pub root: N,
}

/// Builder for creating a test harness with a fluent API.
pub struct HarnessBuilder<N> {
    /// Root node under test.
    root: N,
    /// Viewport size for the harness.
    size: Expanse,
}

impl<N: Node + Loader> HarnessBuilder<N> {
    /// Create a new harness builder with the given root node.
    fn new(root: N) -> Self {
        Self {
            root,
            size: Expanse::new(100, 100), // default size
        }
    }

    /// Set the size of the harness viewport.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Expanse::new(width, height);
        self
    }

    /// Build the harness with the configured settings.
    pub fn build(mut self) -> Result<Harness<N>> {
        let render = NopBackend::new();
        let mut core = Canopy::new();

        <N as Loader>::load(&mut core);
        core.set_root_size(self.size, &mut self.root)?;

        Ok(Harness {
            canopy: core,
            backend: render,
            root: self.root,
        })
    }
}

impl<N: Node + Loader> Harness<N> {
    /// Create a harness builder for constructing a test harness with a fluent API.
    ///
    /// # Example
    /// ```no_run
    /// # use canopy_core::tutils::harness::Harness;
    /// # use canopy_core::{Node, Loader};
    /// # fn example<N: Node + Loader>(node: N) -> canopy_core::Result<()> {
    /// let harness = Harness::builder(node)
    ///     .size(80, 24)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder(root: N) -> HarnessBuilder<N> {
        HarnessBuilder::new(root)
    }

    /// Create a harness using `size` for the root layout.
    ///
    /// This method is kept for backwards compatibility. Consider using `builder()` instead.
    pub fn with_size(mut root: N, size: Expanse) -> Result<Self> {
        let render = NopBackend::new();
        let mut core = Canopy::new();
        <N as Loader>::load(&mut core);
        core.set_root_size(size, &mut root)?;
        Ok(Self {
            canopy: core,
            backend: render,
            root,
        })
    }

    /// Create a harness with a default root size of 100x100.
    ///
    /// This method is kept for backwards compatibility. Consider using `builder()` instead.
    pub fn new(root: N) -> Result<Self> {
        Self::with_size(root, Expanse::new(100, 100))
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
        self.canopy.key(&mut self.root, k)?;
        self.canopy.render(&mut self.backend, &mut self.root)
    }

    /// Render the root node into the harness backend.
    pub fn render(&mut self) -> Result<()> {
        self.canopy.render(&mut self.backend, &mut self.root)
    }

    /// Execute a script on the app under test. The script is compiled and then
    /// executed on the root node, similar to how key bindings work.
    pub fn script(&mut self, script: &str) -> Result<()> {
        let script_id = self.canopy.script_host.compile(script)?;
        let root_id = self.root.id();
        self.canopy.run_script(&mut self.root, root_id, script_id)?;
        self.canopy.render(&mut self.backend, &mut self.root)
    }

    /// Get a BufTest instance that references the current buffer. This provides convenient
    /// access to buffer testing utilities.
    pub fn tbuf(&self) -> BufTest<'_> {
        BufTest::new(self.buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        self as canopy, Context, Layout, Loader, Node, NodeState, Render, StatefulNode,
        derive_commands,
        geom::{Expanse, Line},
    };

    #[derive(StatefulNode)]
    struct TestNode {
        state: NodeState,
    }

    #[derive_commands]
    impl TestNode {
        fn new() -> Self {
            Self {
                state: NodeState::default(),
            }
        }
    }

    impl Node for TestNode {
        fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
            self.fill(sz)?;
            Ok(())
        }

        fn render(&mut self, _ctx: &dyn Context, r: &mut Render) -> Result<()> {
            r.text("base", Line::new(0, 0, 5), "test")?;
            Ok(())
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

        // This test just verifies dump() runs without panicking
        // The actual output goes to stdout
        h.tbuf().dump();

        // Also verify the text was rendered
        assert!(h.tbuf().contains_text("test"));
    }

    #[test]
    fn test_harness_builder() {
        // Test the new builder API
        let mut h = Harness::builder(TestNode::new())
            .size(20, 5)
            .build()
            .unwrap();

        h.render().unwrap();

        // Verify the text was rendered
        assert!(h.tbuf().contains_text("test"));

        // Verify the size was set correctly
        assert_eq!(h.buf().size().w, 20);
        assert_eq!(h.buf().size().h, 5);
    }
}
