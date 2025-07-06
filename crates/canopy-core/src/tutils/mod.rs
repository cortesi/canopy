pub mod buf;
pub mod dummyctx;
pub mod grid;
pub mod harness;
pub mod render;
pub mod ttree;

#[cfg(test)]
mod tests {
    use crate::{self as canopy};

    use crate::{
        Context, Layout, Node, NodeState, Render, Result, StatefulNode, backend::test::TestRender,
        derive_commands, geom::Expanse,
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
                    l.place(ch, vps[i])?;
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
        let mut canopy = canopy::Canopy::new();
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

    #[test]
    fn render_on_focus_change() -> Result<()> {
        let (_, mut tr) = TestRender::create();
        let mut canopy = canopy::Canopy::new();
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
