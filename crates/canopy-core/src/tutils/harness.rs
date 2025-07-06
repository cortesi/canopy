use super::buf;
use super::render::NopBackend;
use crate::{Canopy, Loader};
use crate::{Node, Result, TermBuf, event::key, geom::Expanse};

/// A simple harness that holds a [`Canopy`], a [`DummyBackend`] backend and a
/// root node. Tests drive the UI by sending key events and triggering renders
/// and can then inspect the render buffer.
pub struct Harness<N> {
    core: Canopy,
    render: NopBackend,
    root: N,
}

impl<N: Node + Loader> Harness<N> {
    /// Create a harness using `size` for the root layout.
    pub fn with_size(mut root: N, size: Expanse) -> Result<Self> {
        let render = NopBackend::new();
        let mut core = Canopy::new();

        <N as Loader>::load(&mut core);
        core.set_root_size(size, &mut root)?;

        Ok(Harness { core, render, root })
    }

    /// Create a harness with a default root size of 100x100.
    pub fn new(root: N) -> Result<Self> {
        Self::with_size(root, Expanse::new(100, 100))
    }

    pub fn key<T>(&mut self, k: T) -> Result<()>
    where
        T: Into<key::Key>,
    {
        self.core.key(&mut self.root, k)?;
        self.core.render(&mut self.render, &mut self.root)
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

    /// Execute a script on the app under test. The script is compiled and then
    /// executed on the root node, similar to how key bindings work.
    pub fn script(&mut self, script: &str) -> Result<()> {
        let script_id = self.core.script_host.compile(script)?;
        let root_id = self.root.id();
        self.core.run_script(&mut self.root, root_id, script_id)?;
        self.core.render(&mut self.render, &mut self.root)
    }

    /// Access the current render buffer. Panics if a render has not yet been
    /// performed.
    pub fn buf(&self) -> &TermBuf {
        self.core.buf().expect("render buffer not initialized")
    }

    pub fn expect_contains(&self, txt: &str) {
        assert!(
            buf::contains_text(self.buf(), txt),
            "render buffer missing '{txt}'"
        );
    }

    pub fn expect_highlight(&self, txt: &str) {
        use crate::style::{PartialStyle, solarized};
        let buf = self.buf();

        // Debug helper: if assertion will fail, print what's in the buffer
        if !buf::contains_text_style(buf, txt, &PartialStyle::fg(solarized::BLUE)) {
            eprintln!("Debug: Text '{txt}' not found with blue highlight");
            // First check if the text exists at all
            if buf::contains_text(buf, txt) {
                eprintln!("  Text '{txt}' exists in buffer but without blue highlight!");
            } else {
                eprintln!("  Text '{txt}' not found in buffer at all!");
            }
            eprintln!("Buffer contents:");
            for (i, line) in buf.lines().iter().enumerate() {
                if !line.trim().is_empty() {
                    eprintln!("  Line {}: '{}'", i, line.trim());
                }
            }
        }

        assert!(
            buf::contains_text_style(buf, txt, &PartialStyle::fg(solarized::BLUE)),
            "render buffer missing highlighted '{txt}'"
        );
    }

    /// Dump the current render buffer to the terminal for debugging.
    /// This is useful for visualizing the buffer contents during test development.
    pub fn dump(&self) {
        buf::dump(self.buf());
    }

    /// Alias for dump() to maintain backwards compatibility.
    pub fn dump_buf(&self) {
        self.dump();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{self as canopy};
    use crate::{
        Context, Layout, Loader, Node, NodeState, Render, StatefulNode, derive_commands,
        geom::{Expanse, Line},
    };

    #[derive(StatefulNode)]
    struct TestNode {
        state: NodeState,
    }

    #[derive_commands]
    impl TestNode {
        fn new() -> Self {
            TestNode {
                state: NodeState::default(),
            }
        }
    }

    impl Node for TestNode {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
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
        let mut h = Harness::with_size(TestNode::new(), Expanse::new(10, 3)).unwrap();
        h.render().unwrap();

        // This test just verifies dump() runs without panicking
        // The actual output goes to stdout
        h.dump();

        // Also verify the text was rendered
        assert!(buf::contains_text(h.buf(), "test"));
    }
}
