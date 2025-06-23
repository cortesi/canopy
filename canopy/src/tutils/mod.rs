pub mod ttree;
pub use ttree::*;

use crate::{self as canopy};
use crate::{
    backend::test::TestRender,
    geom::{Direction, Expanse, Rect},
    path::Path,
    widgets::list::ListItem,
    *,
};

// A fixed-size test node
#[derive(Debug, PartialEq, Eq, StatefulNode)]
pub struct TFixed {
    state: NodeState,
    pub w: u16,
    pub h: u16,
}

impl Node for TFixed {
    fn layout(&mut self, l: &Layout, _: Expanse) -> Result<()> {
        let x = Expanse::new(self.w, self.h);
        l.fill(self, x)?;
        Ok(())
    }
}

#[derive_commands]
impl TFixed {
    pub fn new(w: u16, h: u16) -> Self {
        TFixed {
            state: NodeState::default(),
            w,
            h,
        }
    }
}

impl ListItem for TFixed {}

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

pub struct DummyContext {}

impl Context for DummyContext {
    fn is_on_focus_path(&self, _n: &mut dyn Node) -> bool {
        false
    }
    fn is_focused(&self, _n: &dyn Node) -> bool {
        false
    }
    fn focus_area(&self, _root: &mut dyn Node) -> Option<Rect> {
        None
    }
    fn focus_down(&mut self, _root: &mut dyn Node) {}
    fn focus_first(&mut self, _root: &mut dyn Node) {}
    fn focus_left(&mut self, _root: &mut dyn Node) {}
    fn focus_next(&mut self, _root: &mut dyn Node) {}
    fn focus_path(&self, _root: &mut dyn Node) -> Path {
        Path::empty()
    }
    fn focus_prev(&mut self, _root: &mut dyn Node) {}
    fn focus_right(&mut self, _root: &mut dyn Node) {}
    fn focus_up(&mut self, _root: &mut dyn Node) {}
    fn needs_render(&self, _n: &dyn Node) -> bool {
        false
    }
    fn set_focus(&mut self, _n: &mut dyn Node) {}
    fn focus_dir(&mut self, _root: &mut dyn Node, _dir: Direction) {}
    fn taint(&mut self, _n: &mut dyn Node) {}
    fn taint_tree(&mut self, _e: &mut dyn Node) {}

    /// Start the backend renderer.
    fn start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the render backend and exit the process.
    fn exit(&mut self, _code: i32) -> ! {
        panic!("exit in dummy core")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(StatefulNode)]
    struct Block {
        state: NodeState,
        children: Vec<Block>,
        horizontal: bool,
    }

    #[derive_commands]
    impl Block {
        fn new(horizontal: bool) -> Self {
            Block { state: NodeState::default(), children: vec![], horizontal }
        }
    }

    impl Node for Block {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
            if !self.children.is_empty() {
                let vp = self.vp();
                let vps = if self.horizontal {
                    vp.view.split_horizontal(self.children.len() as u16)?
                } else {
                    vp.view.split_vertical(self.children.len() as u16)?
                };
                for (i, ch) in self.children.iter_mut().enumerate() {
                    l.place(ch, vp, vps[i])?;
                }
            }
            Ok(())
        }

        fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
            if self.children.is_empty() {
                r.fill("blue", self.vp().view, 'x')?;
            }
            Ok(())
        }

        fn accept_focus(&mut self) -> bool {
            true
        }

        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            for c in &mut self.children {
                f(c)?;
            }
            Ok(())
        }
    }

    #[test]
    fn block_renders() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;
        canopy.render(&mut tr, &mut root)?;
        assert!(!tr.buf_empty());
        Ok(())
    }

    fn collect_expected(b: &Block, area: Rect, out: &mut Vec<Rect>) -> Result<()> {
        if b.children.is_empty() {
            out.push(area);
        } else {
            let rects = if b.horizontal {
                area.split_horizontal(b.children.len() as u16)?
            } else {
                area.split_vertical(b.children.len() as u16)?
            };
            for (ch, r) in b.children.iter().zip(rects) {
                collect_expected(ch, r, out)?;
            }
        }
        Ok(())
    }

    fn collect_actual(b: &Block, out: &mut Vec<Rect>) {
        if b.children.is_empty() {
            out.push(b.vp().screen_rect());
        } else {
            for ch in &b.children {
                collect_actual(ch, out);
            }
        }
    }

    impl Block {
        fn split(&mut self, _c: &mut dyn Context) -> Result<()> {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            Ok(())
        }
    }

    #[test]
    fn block_progressive_split() -> Result<()> {
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;
        let l = Layout {};
        let mut ctx = DummyContext {};

        // Split the right-most pane repeatedly, emulating the focusgym example.
        root.children[1].split(&mut ctx)?;
        root.children[1].children[1].split(&mut ctx)?;
        root.children[1].children[1].children[1].split(&mut ctx)?;

        root.layout(&l, Expanse::new(20, 10))?;

        let mut expect = Vec::new();
        collect_expected(&root, root.vp().view, &mut expect)?;

        let mut actual = Vec::new();
        collect_actual(&root, &mut actual);

        assert_eq!(actual, expect);
        Ok(())
    }
}
