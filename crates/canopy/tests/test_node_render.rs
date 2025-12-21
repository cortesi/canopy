use canopy::{
    Context, Expanse, Layout, Loader, Node, NodeState, Render, Result, StatefulNode, buf,
    derive_commands, geom, tutils::harness::Harness,
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
        Self {
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
        self.fill(sz)?;
        let vp = self.vp();

        // Place B to fill A's entire space
        l.place(&mut self.node_b, vp.view())?;
        Ok(())
    }
}

#[derive_commands]
impl NodeB {
    fn new() -> Self {
        Self {
            state: NodeState::default(),
        }
    }
}

impl Node for NodeB {
    fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
        self.fit_size(sz, sz);
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
        Self {
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
        self.fill(sz)?;
        // Place A in a 10x5 rectangle
        let node_a_rect = geom::Rect::new(0, 0, 10, 5);
        l.place(&mut self.node_a, node_a_rect)?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.fill("", self.vp().view(), ' ')?;
        Ok(())
    }
}

impl Loader for Root {
    fn load(c: &mut canopy::Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<NodeA>();
        c.add_commands::<NodeB>();
    }
}

#[test]
fn test_simple_node_fill() -> Result<()> {
    let mut h = Harness::builder(Root::new()).size(30, 10).build()?;
    h.render()?;
    h.tbuf().assert_matches(buf![
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
    ]);
    Ok(())
}
