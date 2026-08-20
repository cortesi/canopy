//! Tree traversal and hit-testing integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, NodeId, ViewContext, Widget,
        error::Result,
        geom::{Point, Size},
        path::Path,
        state::NodeName,
        testing::grid::Grid,
    };

    struct TreeWidget {
        name: String,
    }

    impl TreeWidget {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl Widget for TreeWidget {
        fn name(&self) -> NodeName {
            NodeName::convert(&self.name)
        }
    }

    fn build_tree(
        canopy: &mut Canopy,
    ) -> Result<(NodeId, NodeId, NodeId, NodeId, NodeId, NodeId, NodeId)> {
        let root: NodeId = canopy.replace_root(TreeWidget::new("r"))?.into();
        canopy.with_root_context(|context| {
            let ba: NodeId = context.create_detached(TreeWidget::new("ba"))?.into();
            let bb: NodeId = context.create_detached(TreeWidget::new("bb"))?.into();
            let ba_la: NodeId = context.create_detached(TreeWidget::new("ba_la"))?.into();
            let ba_lb: NodeId = context.create_detached(TreeWidget::new("ba_lb"))?.into();
            let bb_la: NodeId = context.create_detached(TreeWidget::new("bb_la"))?.into();
            let bb_lb: NodeId = context.create_detached(TreeWidget::new("bb_lb"))?.into();
            context.set_children_of(root, vec![ba, bb])?;
            context.set_children_of(ba, vec![ba_la, ba_lb])?;
            context.set_children_of(bb, vec![bb_la, bb_lb])?;
            Ok((root, ba, bb, ba_la, ba_lb, bb_la, bb_lb))
        })
    }

    #[test]
    fn preorder_yields_the_tree_in_declaration_order() -> Result<()> {
        let mut canopy = Canopy::new();
        let (root, ..) = build_tree(&mut canopy)?;

        let names = canopy.with_root_view(|context| {
            context
                .preorder(root)
                .map(|node| node_name(context, root, node))
                .collect::<Vec<_>>()
        });
        assert_eq!(names, ["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"]);
        Ok(())
    }

    #[test]
    fn test_node_path() -> Result<()> {
        let mut canopy = Canopy::new();
        let (root, _ba, _bb, ba_la, _ba_lb, _bb_la, _bb_lb) = build_tree(&mut canopy)?;

        canopy.with_root_view(|context| {
            assert_eq!(context.node_path(root, root), Path::new(["r"]));
            assert_eq!(
                context.node_path(root, ba_la),
                Path::new(["r", "ba", "ba_la"])
            );
        });

        Ok(())
    }

    fn node_name(context: &dyn ViewContext, root: NodeId, node: NodeId) -> String {
        context
            .node_path(root, node)
            .pop()
            .expect("node path should contain a name")
    }

    fn locate_name(canopy: &Canopy, root: NodeId, point: Point) -> Result<Option<String>> {
        canopy.with_root_view(|context| {
            context
                .locate(root, point)
                .map(|node| node.map(|node| node_name(context, root, node)))
        })
    }

    #[test]
    fn test_locate_single_cell_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 0, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(10, 10));

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((0, 0), "cell_0_0"),
            ((9, 0), "cell_0_0"),
            ((0, 9), "cell_0_0"),
            ((9, 9), "cell_0_0"),
        ];

        for (point, expected) in test_points {
            let found = locate_name(
                &canopy,
                grid.root,
                Point {
                    x: point.0,
                    y: point.1,
                },
            )?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_2x2_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(20, 20));

        let test_points = vec![
            ((5, 5), "cell_0_0"),
            ((15, 5), "cell_1_0"),
            ((5, 15), "cell_0_1"),
            ((15, 15), "cell_1_1"),
        ];

        for (point, expected) in test_points {
            let found = locate_name(
                &canopy,
                grid.root,
                Point {
                    x: point.0,
                    y: point.1,
                },
            )?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_locate_3x3_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 3)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(30, 30));

        for row in 0..3 {
            for col in 0..3 {
                let x = col as u32 * 10 + 5;
                let y = row as u32 * 10 + 5;
                let expected = format!("cell_{col}_{row}");
                let found = locate_name(&canopy, grid.root, Point { x, y })?;
                assert_eq!(found, Some(expected));
            }
        }

        Ok(())
    }

    #[test]
    fn test_locate_nested_grid() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 2, 2)?;
        let grid_size = grid.expected_size();
        assert_eq!(grid_size, Size::new(40, 40));

        let corner_tests = vec![
            (Point { x: 5, y: 5 }, "cell_0_0"),
            (Point { x: 35, y: 5 }, "cell_3_0"),
            (Point { x: 5, y: 35 }, "cell_0_3"),
            (Point { x: 35, y: 35 }, "cell_3_3"),
        ];

        for (point, expected) in corner_tests {
            let found = locate_name(&canopy, grid.root, point)?;
            assert_eq!(found, Some(expected.to_string()));
        }

        Ok(())
    }

    #[test]
    fn test_grid_boundary_conditions() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 2)?;

        let result = locate_name(&canopy, grid.root, Point { x: 100, y: 100 })?;
        assert_eq!(result, None);

        Ok(())
    }
}
