use canopy::{
    Context, Expanse, Layout, Loader, Node, NodeState, Render, Result, StatefulNode, buf,
    derive_commands, geom,
    tutils::{buf as tutils_buf, harness::Harness},
};

// Define our node types
#[derive(StatefulNode)]
struct NodeB {
    state: NodeState,
}

#[derive(StatefulNode)]
struct NodeA {
    state: NodeState,
    node_b: NodeB,
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    node_a: NodeA,
}

#[derive_commands]
impl NodeA {
    fn new() -> Self {
        NodeA {
            state: NodeState::default(),
            node_b: NodeB::new(),
        }
    }
}

impl Node for NodeA {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.node_b)?;
        Ok(())
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // A fills its available space
        l.fill(self, sz)?;
        let vp = self.vp();

        // Place B to fill A's entire space
        l.place(&mut self.node_b, vp, vp.view())?;
        Ok(())
    }
}

#[derive_commands]
impl NodeB {
    fn new() -> Self {
        NodeB {
            state: NodeState::default(),
        }
    }
}

impl Node for NodeB {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.size(self, sz, sz)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.fill("", self.vp().view(), 'B')?;
        Ok(())
    }
}

#[derive_commands]
impl Root {
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            node_a: NodeA::new(),
        }
    }
}

impl Node for Root {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.node_a)?;
        Ok(())
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        // Root fills the entire screen
        l.fill(self, sz)?;
        let vp = self.vp();

        // Place A in a 10x5 rectangle
        let node_a_rect = geom::Rect::new(0, 0, 10, 5);
        l.place(&mut self.node_a, vp, node_a_rect)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.fill("", self.vp().view(), ' ')?;
        Ok(())
    }
}

impl Loader for Root {
    fn load(c: &mut canopy::Canopy) {
        c.add_commands::<Root>();
        c.add_commands::<NodeA>();
        c.add_commands::<NodeB>();
    }
}

#[test]
fn test_simple_node_fill() -> Result<()> {
    let size = Expanse::new(30, 10);
    let mut h = Harness::with_size(Root::new(), size)?;
    h.render()?;
    let buf = h.buf();
    tutils_buf::assert_matches(
        buf,
        buf![
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "BBBBBBBBBB                    "
            "                              "
            "                              "
            "                              "
            "                              "
            "                              "
        ],
    );
    Ok(())
}
