use super::ttree;
use crate::{
    backend::test::TestRender, event::key, geom::Expanse, Canopy, Loader, Node, Result, TermBuf,
};

/// Run a function on our standard dummy app built from [`ttree`]. This helper
/// is used extensively in unit tests across the codebase.
pub fn run(func: impl FnOnce(&mut Canopy, TestRender, ttree::R) -> Result<()>) -> Result<()> {
    let (_, tr) = TestRender::create();
    let mut root = ttree::R::new();
    let mut c = Canopy::new();

    c.add_commands::<ttree::R>();
    c.add_commands::<ttree::BaLa>();
    c.add_commands::<ttree::BaLb>();
    c.add_commands::<ttree::BbLa>();
    c.add_commands::<ttree::BbLb>();
    c.add_commands::<ttree::Ba>();
    c.add_commands::<ttree::Bb>();

    c.set_root_size(Expanse::new(100, 100), &mut root)?;
    ttree::reset_state();
    func(&mut c, tr, root)
}

/// A simple harness that holds a [`Canopy`], a [`TestRender`] backend and a
/// root node. Tests drive the UI by sending key events and triggering renders
/// and can then inspect the render buffer.
pub struct Harness<N> {
    core: Canopy,
    render: TestRender,
    root: N,
}

impl<N: Node + Loader> Harness<N> {
    /// Create a harness using `size` for the root layout.
    pub fn with_size(mut root: N, size: Expanse) -> Result<Self> {
        let (_, tr) = TestRender::create();
        let mut core = Canopy::new();

        <N as Loader>::load(&mut core);
        core.set_root_size(size, &mut root)?;

        Ok(Harness {
            core,
            render: tr,
            root,
        })
    }

    /// Create a harness with a default root size of 100x100.
    pub fn new(root: N) -> Result<Self> {
        Self::with_size(root, Expanse::new(100, 100))
    }

    pub fn key<T>(&mut self, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.core.key(&mut self.root, k)
    }

    pub fn render(&mut self) -> Result<()> {
        self.core.render(&mut self.render, &mut self.root)
    }

    pub fn canopy(&mut self) -> &mut Canopy {
        &mut self.core
    }

    pub fn root(&mut self) -> &mut N {
        &mut self.root
    }

    pub fn backend(&mut self) -> &mut TestRender {
        &mut self.render
    }

    /// Access the current render buffer. Panics if a render has not yet been
    /// performed.
    pub fn buf(&self) -> &TermBuf {
        self.core
            .render_buf()
            .expect("render buffer not initialized")
    }
}
