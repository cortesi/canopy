//! Integration tests for tree traversal.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, Core, NodeId, ViewContext, derive_commands,
        error::{Error, Result},
        geom::{Direction, Expanse, Point},
        layout::{Layout, Sizing},
        path::Path,
        render::Render,
        state::NodeName,
        testing::grid::Grid,
        widget::Widget,
    };

    #[derive(Debug, Clone, PartialEq)]
    enum Walk<T> {
        Skip,
        Handle(T),
        Continue,
    }

    struct TreeWidget {
        name: String,
    }

    #[derive_commands]
    impl TreeWidget {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl Widget for TreeWidget {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert(&self.name)
        }
    }

    fn build_tree(
        core: &mut Core,
    ) -> Result<(NodeId, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId)> {
        core.set_widget(core.root, TreeWidget::new("r"));
        let ba = core.add(TreeWidget::new("ba"));
        let bb = core.add(TreeWidget::new("bb"));
        let ba_la = core.add(TreeWidget::new("ba_la"));
        let ba_lb = core.add(TreeWidget::new("ba_lb"));
        let bb_la = core.add(TreeWidget::new("bb_la"));
        let bb_lb = core.add(TreeWidget::new("bb_lb"));
        core.set_children(core.root, vec![ba, bb])?;
        core.set_children(ba, vec![ba_la, ba_lb])?;
        core.set_children(bb, vec![bb_la, bb_lb])?;
        Ok((core.root, ba, bb, ba_la, ba_lb, bb_la, bb_lb))
    }

    fn preorder<T>(
        core: &Core,
        root: NodeId,
        f: &mut dyn FnMut(NodeId) -> Result<Walk<T>>,
    ) -> Result<Walk<T>> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            match f(id)? {
                Walk::Handle(v) => return Ok(Walk::Handle(v)),
                Walk::Skip => continue,
                Walk::Continue => {}
            }
            if let Some(node) = core.nodes.get(id) {
                for child in node.children.iter().rev() {
                    stack.push(*child);
                }
            }
        }
        Ok(Walk::Continue)
    }

    fn postorder_visit<T>(
        core: &Core,
        node_id: NodeId,
        f: &mut dyn FnMut(NodeId) -> Result<Walk<T>>,
    ) -> Result<Walk<T>> {
        let mut skip_branch = false;
        if let Some(node) = core.nodes.get(node_id) {
            for child in node.children.clone() {
                match postorder_visit(core, child, f)? {
                    Walk::Continue => {}
                    Walk::Handle(v) => return Ok(Walk::Handle(v)),
                    Walk::Skip => {
                        skip_branch = true;
                        break;
                    }
                }
            }
        }

        match f(node_id)? {
            Walk::Continue if skip_branch => Ok(Walk::Skip),
            res => Ok(res),
        }
    }

    fn postorder<T>(
        core: &Core,
        root: NodeId,
        f: &mut dyn FnMut(NodeId) -> Result<Walk<T>>,
    ) -> Result<Walk<T>> {
        postorder_visit(core, root, f)
    }

    #[test]
    fn test_node_path() -> Result<()> {
        let mut canopy = Canopy::new();
        let (root, _ba, _bb, ba_la, _ba_lb, _bb_la, _bb_lb) = build_tree(&mut canopy.core)?;

        assert_eq!(canopy.core.node_path(root, root), Path::new(&["r"]));
        assert_eq!(
            canopy.core.node_path(root, ba_la),
            Path::new(&["r", "ba", "ba_la"])
        );

        Ok(())
    }

    fn vc(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn test_preorder() -> Result<()> {
        fn trigger(name: &str, func: &Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut canopy = Canopy::new();
            let (root, _ba, _bb, _ba_la, _ba_lb, _bb_la, _bb_lb) =
                build_tree(&mut canopy.core).unwrap();
            let mut v = Vec::new();
            let res = preorder(&canopy.core, root, &mut |id| -> Result<Walk<()>> {
                let name_str = canopy.core.nodes[id].name.to_string();
                v.push(name_str.clone());
                if name_str == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        assert_eq!(
            trigger("never", &Ok(Walk::Skip)),
            (
                vc(&["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"]),
                Ok(Walk::Continue)
            )
        );

        assert_eq!(
            trigger("ba", &Ok(Walk::Skip)),
            (vc(&["r", "ba", "bb", "bb_la", "bb_lb"]), Ok(Walk::Continue))
        );
        assert_eq!(
            trigger("r", &Ok(Walk::Skip)),
            (vc(&["r"]), Ok(Walk::Continue))
        );

        assert_eq!(
            trigger("ba", &Ok(Walk::Handle(()))),
            (vc(&["r", "ba"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("ba_la", &Ok(Walk::Handle(()))),
            (vc(&["r", "ba", "ba_la"]), Ok(Walk::Handle(())))
        );

        assert_eq!(
            trigger("ba_la", &Err(Error::NoResult)),
            (vc(&["r", "ba", "ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("r", &Err(Error::NoResult)),
            (vc(&["r"]), Err(Error::NoResult))
        );

        Ok(())
    }

    #[test]
    fn test_postorder() -> Result<()> {
        fn trigger(name: &str, func: &Result<Walk<()>>) -> (Vec<String>, Result<Walk<()>>) {
            let mut canopy = Canopy::new();
            let (root, _ba, _bb, _ba_la, _ba_lb, _bb_la, _bb_lb) =
                build_tree(&mut canopy.core).unwrap();
            let mut v = Vec::new();
            let res = postorder(&canopy.core, root, &mut |id| -> Result<Walk<()>> {
                let name_str = canopy.core.nodes[id].name.to_string();
                v.push(name_str.clone());
                if name_str == name {
                    func.clone()
                } else {
                    Ok(Walk::Continue)
                }
            });
            (v, res)
        }

        assert_eq!(
            trigger("ba_la", &Ok(Walk::Skip)),
            (vc(&["ba_la", "ba", "r"]), Ok(Walk::Skip))
        );

        assert_eq!(
            trigger("ba_lb", &Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );
        assert_eq!(
            trigger("r", &Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("bb", &Ok(Walk::Skip)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"]),
                Ok(Walk::Skip)
            )
        );
        assert_eq!(
            trigger("ba", &Ok(Walk::Skip)),
            (vc(&["ba_la", "ba_lb", "ba", "r"]), Ok(Walk::Skip))
        );

        assert_eq!(
            trigger("ba_la", &Ok(Walk::Handle(()))),
            (vc(&["ba_la"]), Ok(Walk::Handle(())))
        );
        assert_eq!(
            trigger("bb", &Ok(Walk::Handle(()))),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Ok(Walk::Handle(()))
            )
        );

        assert_eq!(
            trigger("ba_la", &Err(Error::NoResult)),
            (vc(&["ba_la"]), Err(Error::NoResult))
        );
        assert_eq!(
            trigger("bb", &Err(Error::NoResult)),
            (
                vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"]),
                Err(Error::NoResult)
            )
        );

        Ok(())
    }

    fn attach_grid(core: &mut Core, grid_root: NodeId, size: Expanse) -> Result<()> {
        core.set_children(core.root, vec![grid_root])?;
        core.with_layout_of(core.root, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(grid_root, |layout| {
            layout.width = Sizing::Flex(1);
            layout.height = Sizing::Flex(1);
        })?;
        core.update_layout(size)?;
        Ok(())
    }

    #[test]
    fn test_locate_single_cell_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 0, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(10, 10));
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((0, 0), "cell_0_0"),
            ((9, 0), "cell_0_0"),
            ((0, 9), "cell_0_0"),
            ((9, 9), "cell_0_0"),
        ];

        for (point, expected) in test_points {
            let found = canopy
                .core
                .locate_node(
                    grid.root,
                    Point {
                        x: point.0,
                        y: point.1,
                    },
                )?
                .and_then(|id| canopy.core.nodes.get(id).map(|n| n.name.to_string()));
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_2x2_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(20, 20));
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((15, 5), "cell_1_0"),
            ((5, 15), "cell_0_1"),
            ((15, 15), "cell_1_1"),
        ];

        for (point, expected) in test_points {
            let found = canopy
                .core
                .locate_node(
                    grid.root,
                    Point {
                        x: point.0,
                        y: point.1,
                    },
                )?
                .and_then(|id| canopy.core.nodes.get(id).map(|n| n.name.to_string()));
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_3x3_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 1, 3)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(30, 30));
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        for row in 0..3 {
            for col in 0..3 {
                let x = col as u32 * 10 + 5;
                let y = row as u32 * 10 + 5;
                let expected = format!("cell_{col}_{row}");
                let found = canopy
                    .core
                    .locate_node(grid.root, Point { x, y })?
                    .and_then(|id| canopy.core.nodes.get(id).map(|n| n.name.to_string()));
                assert_eq!(found, Some(expected));
            }
        }

        Ok(())
    }

    #[test]
    fn test_locate_nested_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 2, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Expanse::new(40, 40));
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        let corner_tests = vec![
            (Point { x: 5, y: 5 }, "cell_0_0"),
            (Point { x: 35, y: 5 }, "cell_3_0"),
            (Point { x: 5, y: 35 }, "cell_0_3"),
            (Point { x: 35, y: 35 }, "cell_3_3"),
        ];

        for (point, expected) in corner_tests {
            let found = canopy
                .core
                .locate_node(grid.root, point)?
                .and_then(|id| canopy.core.nodes.get(id).map(|n| n.name.to_string()));
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_grid_boundary_conditions() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 1, 2)?;
        let grid_size = grid.expected_size();
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        let result = canopy
            .core
            .locate_node(grid.root, Point { x: 100, y: 100 })?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_focus_dir_navigation() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy.core, 1, 2)?;
        let grid_size = grid.expected_size();
        attach_grid(&mut canopy.core, grid.root, grid_size)?;

        let get_focused_cell = |canopy: &Canopy| -> Option<String> {
            canopy
                .core
                .focus
                .and_then(|id| canopy.core.nodes.get(id).map(|n| n.name.to_string()))
        };

        canopy.core.focus_first(grid.root);
        assert_eq!(get_focused_cell(&canopy), Some("cell_0_0".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Right);
        assert_eq!(get_focused_cell(&canopy), Some("cell_1_0".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Down);
        assert_eq!(get_focused_cell(&canopy), Some("cell_1_1".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Left);
        assert_eq!(get_focused_cell(&canopy), Some("cell_0_1".to_string()));

        canopy.core.focus_dir(grid.root, Direction::Up);
        assert_eq!(get_focused_cell(&canopy), Some("cell_0_0".to_string()));

        Ok(())
    }
}
