mod grid;
pub mod harness;
pub mod pty;
pub mod ttree;
use crate::{self as canopy};
use crate::{
    geom::{Direction, Expanse},
    path::Path,
    *,
};
pub use grid::{Grid, GridNode};
pub use harness::*;
pub use pty::*;
pub use ttree::*;

// A fixed-size test node
#[derive(Debug, PartialEq, Eq, StatefulNode)]
pub struct TFixed {
    state: NodeState,
    pub w: u32,
    pub h: u32,
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
    pub fn new(w: u32, h: u32) -> Self {
        TFixed {
            state: NodeState::default(),
            w,
            h,
        }
    }
}

pub struct DummyContext {}

impl Context for DummyContext {
    fn is_on_focus_path(&self, _n: &mut dyn Node) -> bool {
        false
    }
    fn is_focused(&self, _n: &dyn Node) -> bool {
        false
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
    fn set_focus(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn focus_dir(&mut self, _root: &mut dyn Node, _dir: Direction) {}
    fn scroll_to(&mut self, _n: &mut dyn Node, _x: u32, _y: u32) -> bool {
        false
    }
    fn scroll_by(&mut self, _n: &mut dyn Node, _x: i32, _y: i32) -> bool {
        false
    }
    fn page_up(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn page_down(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_up(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_down(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_left(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_right(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
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

    fn current_focus_gen(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::{
        Canopy,
        backend::test::{CanvasRender, TestRender},
    };

    #[derive(StatefulNode)]
    struct Block {
        state: NodeState,
        children: Vec<Block>,
        horizontal: bool,
    }

    #[derive_commands]
    impl Block {
        fn new(horizontal: bool) -> Self {
            Block {
                state: NodeState::default(),
                children: vec![],
                horizontal,
            }
        }

        /// Split this block into two children, toggling orientation like the
        /// `focusgym` example.
        fn split(&mut self) {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
        }

        /// Split using a `Context` to replicate the behaviour of the
        /// `focusgym` example, which taints the tree and moves focus to the
        /// new pane.
        fn split_ctx(&mut self, c: &mut dyn Context) {
            self.split();
            c.taint_tree(self);
            c.focus_next(self);
        }
    }

    impl Node for Block {
        fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
            l.fill(self, sz)?;
            if !self.children.is_empty() {
                let vp = self.vp();
                let vps = if self.horizontal {
                    vp.view().split_horizontal(self.children.len() as u32)?
                } else {
                    vp.view().split_vertical(self.children.len() as u32)?
                };
                for (i, ch) in self.children.iter_mut().enumerate() {
                    l.place(ch, vp, vps[i])?;
                }
            }
            Ok(())
        }

        fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
            if self.children.is_empty() {
                r.fill("blue", self.vp().view(), 'x')?;
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

    fn gather_leaves<'a>(b: &'a Block, out: &mut Vec<&'a Block>) {
        if b.children.is_empty() {
            out.push(b);
        } else {
            for c in &b.children {
                gather_leaves(c, out);
            }
        }
    }

    fn expected_rects(b: &Block, area: Rect) -> Result<Vec<Rect>> {
        if b.children.is_empty() {
            return Ok(vec![area]);
        }
        let vps = if b.horizontal {
            area.split_horizontal(b.children.len() as u32)?
        } else {
            area.split_vertical(b.children.len() as u32)?
        };
        let mut ret = Vec::new();
        for (child, rect) in b.children.iter().zip(vps.into_iter()) {
            ret.extend(expected_rects(child, rect)?);
        }
        Ok(ret)
    }

    #[test]
    fn focusgym_layout() -> Result<()> {
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };
        let l = Layout {};

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;

        // Split the right-hand block, then split its bottom block
        root.children[1].split();
        root.children[1].children[1].split();

        root.layout(&l, Expanse::new(20, 10))?;

        let mut leaves = Vec::new();
        gather_leaves(&root, &mut leaves);
        let expect = expected_rects(&root, root.vp().screen_rect())?;

        let got: Vec<Rect> = leaves.iter().map(|b| b.vp().screen_rect()).collect();
        assert_eq!(got, expect);

        Ok(())
    }

    #[test]
    fn focusgym_split_right_render() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };
        let l = Layout {};

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;
        canopy.set_focus(&mut root.children[1]);
        root.children[1].split_ctx(&mut canopy);

        root.layout(&l, Expanse::new(20, 10))?;

        let mut leaves = Vec::new();
        gather_leaves(&root, &mut leaves);
        let expect = expected_rects(&root, root.vp().screen_rect())?;
        let got: Vec<Rect> = leaves.iter().map(|b| b.vp().screen_rect()).collect();
        assert_eq!(got, expect);

        canopy.render(&mut tr, &mut root)?;
        assert!(!tr.buf_empty());

        Ok(())
    }

    #[test]
    fn focusgym_nested_layout() -> Result<()> {
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };
        let l = Layout {};

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;

        // Split the right-hand block, then split its bottom block and then its
        // left-most grandchild. This nests three levels of panes.
        root.children[1].split();
        root.children[1].children[1].split();
        root.children[1].children[1].children[0].split();

        root.layout(&l, Expanse::new(20, 10))?;

        let mut leaves = Vec::new();
        gather_leaves(&root, &mut leaves);
        let expect = expected_rects(&root, root.vp().screen_rect())?;

        let got: Vec<Rect> = leaves.iter().map(|b| b.vp().screen_rect()).collect();
        assert_eq!(got, expect);

        Ok(())
    }

    #[test]
    fn focusgym_nested_render() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };
        let l = Layout {};

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;
        canopy.set_focus(&mut root.children[1]);
        root.children[1].split_ctx(&mut canopy);
        canopy.set_focus(&mut root.children[1].children[1]);
        root.children[1].children[1].split_ctx(&mut canopy);
        canopy.set_focus(&mut root.children[1].children[1].children[0]);
        root.children[1].children[1].children[0].split_ctx(&mut canopy);

        root.layout(&l, Expanse::new(20, 10))?;

        let mut leaves = Vec::new();
        gather_leaves(&root, &mut leaves);
        let expect = expected_rects(&root, root.vp().screen_rect())?;
        let got: Vec<Rect> = leaves.iter().map(|b| b.vp().screen_rect()).collect();
        assert_eq!(got, expect);

        canopy.render(&mut tr, &mut root)?;
        assert!(!tr.buf_empty());

        Ok(())
    }

    #[test]
    fn focusgym_canvas_render() -> Result<()> {
        #[derive(StatefulNode)]
        struct Root {
            state: NodeState,
            child: Block,
        }

        #[derive_commands]
        impl Root {
            fn new() -> Self {
                Root {
                    state: NodeState::default(),
                    child: Block {
                        state: NodeState::default(),
                        children: vec![Block::new(false), Block::new(false)],
                        horizontal: true,
                    },
                }
            }
        }

        impl Node for Root {
            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.child)
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                l.fill(self, sz)?;
                let vp = self.vp();
                let parts = vp.view().split_horizontal(2)?;
                l.place(&mut self.child, vp, parts[1])?;
                Ok(())
            }
        }

        let size = Expanse::new(20, 10);
        let (buf, mut cr) = CanvasRender::create(size);
        let mut canopy = Canopy::new();
        let mut root = Root::new();
        let l = Layout {};

        canopy.set_root_size(size, &mut root)?;
        canopy.set_focus(&mut root.child.children[1]);
        root.child.children[1].split_ctx(&mut canopy);
        canopy.set_focus(&mut root.child.children[1].children[1]);
        root.child.children[1].children[1].split_ctx(&mut canopy);
        canopy.set_focus(&mut root.child.children[1].children[1].children[0]);
        root.child.children[1].children[1].children[0].split_ctx(&mut canopy);

        root.layout(&l, size)?;

        let mut leaves = Vec::new();
        gather_leaves(&root.child, &mut leaves);
        let expect = expected_rects(&root.child, root.child.vp().screen_rect())?;
        let got: Vec<Rect> = leaves.iter().map(|b| b.vp().screen_rect()).collect();
        assert_eq!(got, expect);

        canopy.render(&mut cr, &mut root)?;
        let canvas = buf.lock().unwrap();
        for y in 0..size.h {
            for x in 0..size.w {
                let ch = canvas.cells[y as usize][x as usize];
                let inside = expect.iter().any(|r| r.contains_point((x, y)));
                if inside {
                    assert_eq!(ch, 'x');
                } else {
                    assert_eq!(ch, '\0');
                }
            }
        }

        Ok(())
    }

    #[test]
    fn render_on_focus_change() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = Canopy::new();
        let mut root = Block {
            state: NodeState::default(),
            children: vec![Block::new(false), Block::new(false)],
            horizontal: true,
        };

        canopy.set_root_size(Expanse::new(20, 10), &mut root)?;
        canopy.render(&mut tr, &mut root)?;
        tr.text.lock().unwrap().text.clear();

        canopy.focus_next(&mut root);
        canopy.render(&mut tr, &mut root)?;
        assert!(tr.buf_empty());

        Ok(())
    }
}
