use canopy::{
    Context, Expanse, Layout, Loader, Node, NodeState, Render, Result, StatefulNode,
    commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    geom::{Point, Rect},
    state::NodeName,
    tutils::Harness,
};

// Big node that expands to twice its given size
struct Big {
    state: NodeState,
}

impl Big {
    fn new() -> Self {
        Big {
            state: NodeState::default(),
        }
    }
}

impl StatefulNode for Big {
    fn name(&self) -> NodeName {
        NodeName::convert("big")
    }

    fn state(&self) -> &NodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }
}

impl CommandNode for Big {
    fn commands() -> Vec<CommandSpec> {
        vec![]
    }

    fn dispatch(&mut self, _c: &mut dyn Context, _cmd: &CommandInvocation) -> Result<ReturnValue> {
        Ok(ReturnValue::Void)
    }
}

impl Node for Big {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.fill(self, Expanse::new(sz.w * 2, sz.h * 2))
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.fill("", self.vp().canvas().rect(), 'x')
    }
}

// Root node that places Big child in bottom-right corner
struct Root {
    state: NodeState,
    child: Big,
}

impl Root {
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            child: Big::new(),
        }
    }
}

impl StatefulNode for Root {
    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }

    fn state(&self) -> &NodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }
}

impl CommandNode for Root {
    fn commands() -> Vec<CommandSpec> {
        vec![]
    }

    fn dispatch(&mut self, _c: &mut dyn Context, _cmd: &CommandInvocation) -> Result<ReturnValue> {
        Ok(ReturnValue::Void)
    }
}

impl Node for Root {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.fill(self, sz)?;
        let vp = self.vp();
        let loc = Rect::new(sz.w.saturating_sub(1), sz.h.saturating_sub(1), sz.w, sz.h);
        l.place(&mut self.child, vp, loc)?;
        Ok(())
    }
}

impl Loader for Root {
    fn load(_c: &mut canopy::Canopy) {
        // No commands to load
    }
}

#[test]
fn child_clamped_to_parent() -> Result<()> {
    let size = Expanse::new(4, 4);
    let root = Root::new();
    let mut h = Harness::with_size(root, size)?;
    h.render()?;

    // The child (Big) node should be placed in bottom-right corner
    // Big expands to 2x2 when given 1x1, so it should be clamped to parent
    // We expect the bottom-right corner to have 'x'
    let buf = h.buf();

    // Check that only the bottom-right corner has 'x'
    for y in 0..size.h {
        for x in 0..size.w {
            let cell = buf.get(Point { x, y }).unwrap();
            // The child is placed at (3,3) with size (1,1) but expands to (2,2)
            // So it should be clamped to just the bottom-right corner
            if x == 3 && y == 3 {
                assert_eq!(cell.ch, 'x', "Expected 'x' at ({x}, {y})");
            } else {
                assert_eq!(cell.ch, ' ', "Expected ' ' at ({x}, {y})");
            }
        }
    }
    Ok(())
}
