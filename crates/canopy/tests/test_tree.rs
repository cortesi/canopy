use canopy::path::Path;
use canopy::tree::*;
use canopy::*;

struct TreeLeaf {
    state: NodeState,
    name_str: String,
}

impl TreeLeaf {
    fn new(name: &str) -> Self {
        TreeLeaf {
            state: NodeState::default(),
            name_str: name.to_string(),
        }
    }
}

impl Node for TreeLeaf {}

#[derive_commands]
impl TreeLeaf {}

impl StatefulNode for TreeLeaf {
    fn name(&self) -> NodeName {
        NodeName::convert(&self.name_str)
    }

    fn state(&self) -> &NodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }
}

struct TreeBranch {
    state: NodeState,
    name_str: String,
    la: TreeLeaf,
    lb: TreeLeaf,
}

impl TreeBranch {
    fn new(name: &str, la_name: &str, lb_name: &str) -> Self {
        TreeBranch {
            state: NodeState::default(),
            name_str: name.to_string(),
            la: TreeLeaf::new(la_name),
            lb: TreeLeaf::new(lb_name),
        }
    }
}

impl Node for TreeBranch {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.la)?;
        f(&mut self.lb)?;
        Ok(())
    }
}

#[derive_commands]
impl TreeBranch {}

impl StatefulNode for TreeBranch {
    fn name(&self) -> NodeName {
        NodeName::convert(&self.name_str)
    }

    fn state(&self) -> &NodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }
}

struct TreeRoot {
    state: NodeState,
    ba: TreeBranch,
    bb: TreeBranch,
}

impl TreeRoot {
    fn new() -> Self {
        TreeRoot {
            state: NodeState::default(),
            ba: TreeBranch::new("ba", "ba_la", "ba_lb"),
            bb: TreeBranch::new("bb", "bb_la", "bb_lb"),
        }
    }
}

impl Node for TreeRoot {
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.ba)?;
        f(&mut self.bb)?;
        Ok(())
    }
}

#[derive_commands]
impl TreeRoot {}

impl StatefulNode for TreeRoot {
    fn name(&self) -> NodeName {
        NodeName::convert("r")
    }

    fn state(&self) -> &NodeState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut NodeState {
        &mut self.state
    }
}

#[test]
fn test_node_path() -> Result<()> {
    let mut root = TreeRoot::new();

    assert_eq!(node_path(&root.id(), &mut root), Path::new(&["r"]));
    assert_eq!(
        node_path(&root.ba.la.id(), &mut root),
        Path::new(&["r", "ba", "ba_la"])
    );

    Ok(())
}

/// Tiny helper to turn arrays into owned String vecs to ease comparison.
fn vc(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

#[test]
fn test_preorder() -> Result<()> {
    fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
        let mut v: Vec<String> = vec![];
        let mut root = TreeRoot::new();
        let res = preorder(&mut root, &mut |x| -> Result<Walk<()>> {
            v.push(x.name().to_string());
            if x.name() == name {
                func.clone()
            } else {
                Ok(Walk::Continue)
            }
        });
        (v, res)
    }

    assert_eq!(
        trigger("never", Ok(Walk::Skip)),
        (
            vc(&["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"]),
            Ok(Walk::Continue)
        )
    );

    // Skip
    assert_eq!(
        trigger("ba", Ok(Walk::Skip)),
        (vc(&["r", "ba", "bb", "bb_la", "bb_lb"]), Ok(Walk::Continue))
    );
    assert_eq!(
        trigger("r", Ok(Walk::Skip)),
        (vc(&["r"]), Ok(Walk::Continue))
    );

    // Handle
    assert_eq!(
        trigger("ba", Ok(Walk::Handle(()))),
        (vc(&["r", "ba"]), Ok(Walk::Handle(())))
    );
    assert_eq!(
        trigger("ba_la", Ok(Walk::Handle(()))),
        (vc(&["r", "ba", "ba_la"]), Ok(Walk::Handle(())))
    );

    // Error
    assert_eq!(
        trigger("ba_la", Err(Error::NoResult)),
        (vc(&["r", "ba", "ba_la"]), Err(Error::NoResult))
    );
    assert_eq!(
        trigger("r", Err(Error::NoResult)),
        (vc(&["r"]), Err(Error::NoResult))
    );

    Ok(())
}

#[test]
fn test_postorder() -> Result<()> {
    fn trigger(name: &str, func: Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
        let mut v: Vec<String> = vec![];
        let mut root = TreeRoot::new();
        let res = postorder(&mut root, &mut |x| -> Result<Walk<()>> {
            v.push(x.name().to_string());
            if x.name() == name {
                func.clone()
            } else {
                Ok(Walk::Continue)
            }
        });
        (v, res)
    }

    // Skip
    assert_eq!(
        trigger("ba_la", Ok(Walk::Skip)),
        (vc(&["ba_la", "ba", "r"]), Ok(Walk::Skip))
    );

    assert_eq!(
        trigger("ba_lb", Ok(Walk::Skip)),
        (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
    );
    assert_eq!(
        trigger("r", Ok(Walk::Skip)),
        (
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
            Ok(Walk::Skip)
        )
    );
    assert_eq!(
        trigger("bb", Ok(Walk::Skip)),
        (
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
            Ok(Walk::Skip)
        )
    );
    assert_eq!(
        trigger("ba", Ok(Walk::Skip)),
        (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
    );

    // Handle
    assert_eq!(
        trigger("ba_la", Ok(Walk::Handle(()))),
        (vc(&["ba_la"]), Ok(Walk::Handle(())))
    );
    assert_eq!(
        trigger("bb", Ok(Walk::Handle(()))),
        (
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
            Ok(Walk::Handle(()))
        )
    );

    // Error
    assert_eq!(
        trigger("ba_la", Err(Error::NoResult)),
        (vc(&["ba_la"]), Err(Error::NoResult))
    );
    assert_eq!(
        trigger("bb", Err(Error::NoResult)),
        (
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
            Err(Error::NoResult)
        )
    );

    Ok(())
}
