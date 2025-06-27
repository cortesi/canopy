use super::ttree;
use crate::{
    backend::test::TestRender, event::key, geom::Expanse, Canopy, Loader, Node, Result, TermBuf,
};
use std::time::{Duration, Instant};

/// Run a function on our standard dummy app.
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

/// A thin wrapper around [`Canopy`] that exposes a limited public API suitable
/// for driving tests.
pub struct Harness<'a> {
    core: &'a mut Canopy,
}

impl<'a> Harness<'a> {
    pub fn key<T>(&mut self, root: &mut dyn Node, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.core.key(root, k)
    }

    /// Version of [`key`] that fails the test if processing takes longer than
    /// `timeout`.
    pub fn key_timeout<T>(&mut self, root: &mut dyn Node, k: T, timeout: Duration) -> Result<()>
    where
        T: Into<key::Key>,
    {
        let start = Instant::now();
        let ret = self.key(root, k);
        if start.elapsed() > timeout {
            panic!("key event timed out");
        }
        ret
    }

    pub fn render(&mut self, r: &mut TestRender, root: &mut dyn Node) -> Result<()> {
        self.core.render(r, root)
    }

    /// Version of [`render`] that fails the test if processing takes longer than
    /// `timeout`.
    pub fn render_timeout(
        &mut self,
        r: &mut TestRender,
        root: &mut dyn Node,
        timeout: Duration,
    ) -> Result<()> {
        let start = Instant::now();
        let ret = self.render(r, root);
        if start.elapsed() > timeout {
            panic!("render timed out");
        }
        ret
    }

    pub fn canopy(&mut self) -> &mut Canopy {
        self.core
    }

    /// Access the current render buffer from the underlying [`Canopy`] core.
    pub fn buffer(&self) -> Option<&TermBuf> {
        self.core.termbuf()
    }
}

/// Run a function on a provided root node using the test render backend.
///
/// The root node must implement [`Loader`] so that command sets can be loaded
/// for the test environment. The node is laid out with a default size before
/// the supplied closure is executed.
pub fn run_root_with_size<N>(
    mut root: N,
    size: Expanse,
    func: impl FnOnce(&mut Harness<'_>, &mut TestRender, &mut N) -> Result<()>,
) -> Result<()>
where
    N: Node + Loader,
{
    let (_, mut tr) = TestRender::create();
    let mut c = Canopy::new();

    <N as Loader>::load(&mut c);
    c.set_root_size(size, &mut root)?;

    let mut h = Harness { core: &mut c };
    func(&mut h, &mut tr, &mut root)
}

pub fn run_root<N>(
    root: N,
    func: impl FnOnce(&mut Harness<'_>, &mut TestRender, &mut N) -> Result<()>,
) -> Result<()>
where
    N: Node + Loader,
{
    run_root_with_size(root, Expanse::new(100, 100), func)
}
