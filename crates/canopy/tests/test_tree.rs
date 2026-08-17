//! Integration tests for tree traversal.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, FocusScope, NodeId, ViewContext, Widget, derive_commands,
        error::{Error, Result},
        geom::{Direction, Point, Size},
        path::Path,
        render::Render,
        state::NodeName,
        testing::grid::Grid,
    };

    #[derive(Debug, Clone, PartialEq)]
    enum Walk<T> {
        Skip,
        Handle(T),
        Continue,
    }

    #[derive(Clone, Copy)]
    enum TriggerOutcome {
        Skip,
        Handle,
        NoResult,
    }

    fn outcome_result(outcome: TriggerOutcome) -> Result<Walk<()>> {
        match outcome {
            TriggerOutcome::Skip => Ok(Walk::Skip),
            TriggerOutcome::Handle => Ok(Walk::Handle(())),
            TriggerOutcome::NoResult => Err(Error::Internal("no result".into())),
        }
    }

    fn assert_walk_result(actual: Result<Walk<()>>, expected: Result<Walk<()>>) {
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
            (Err(Error::Internal(actual)), Err(Error::Internal(expected))) => {
                assert_eq!(actual, expected);
            }
            (Err(actual), Err(expected)) => {
                panic!("expected Err({expected}), got Err({actual})");
            }
            (Ok(actual), Err(expected)) => {
                panic!("expected Err({expected}), got Ok({actual:?})");
            }
            (Err(actual), Ok(expected)) => {
                panic!("expected Ok({expected:?}), got Err({actual})");
            }
        }
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

    fn preorder<T>(
        core: &dyn ViewContext,
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
            for child in core.children_of(id).into_iter().rev() {
                stack.push(child);
            }
        }
        Ok(Walk::Continue)
    }

    fn postorder_visit<T>(
        core: &dyn ViewContext,
        node_id: NodeId,
        f: &mut dyn FnMut(NodeId) -> Result<Walk<T>>,
    ) -> Result<Walk<T>> {
        let mut skip_branch = false;
        for child in core.children_of(node_id) {
            match postorder_visit(core, child, f)? {
                Walk::Continue => {}
                Walk::Handle(v) => return Ok(Walk::Handle(v)),
                Walk::Skip => {
                    skip_branch = true;
                    break;
                }
            }
        }

        match f(node_id)? {
            Walk::Continue if skip_branch => Ok(Walk::Skip),
            res => Ok(res),
        }
    }

    fn postorder<T>(
        core: &dyn ViewContext,
        root: NodeId,
        f: &mut dyn FnMut(NodeId) -> Result<Walk<T>>,
    ) -> Result<Walk<T>> {
        postorder_visit(core, root, f)
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

    fn vc(a: &[&str]) -> Vec<String> {
        a.iter().map(|x| x.to_string()).collect()
    }

    fn node_name(context: &dyn ViewContext, root: NodeId, node: NodeId) -> String {
        context
            .node_path(root, node)
            .pop()
            .expect("node path should contain a name")
    }

    #[test]
    fn test_preorder() -> Result<()> {
        fn trigger(name: &str, outcome: TriggerOutcome) -> (Vec<String>, Result<Walk<()>>) {
            let mut canopy = Canopy::new();
            let (root, _ba, _bb, _ba_la, _ba_lb, _bb_la, _bb_lb) = build_tree(&mut canopy).unwrap();
            let mut v = Vec::new();
            let res = canopy.with_root_view(|context| {
                preorder(context, root, &mut |id| -> Result<Walk<()>> {
                    let name_str = node_name(context, root, id);
                    v.push(name_str.clone());
                    if name_str == name {
                        outcome_result(outcome)
                    } else {
                        Ok(Walk::Continue)
                    }
                })
            });
            (v, res)
        }

        let (visited, result) = trigger("never", TriggerOutcome::Skip);
        assert_eq!(
            visited,
            vc(&["r", "ba", "ba_la", "ba_lb", "bb", "bb_la", "bb_lb"])
        );
        assert_walk_result(result, Ok(Walk::Continue));

        let (visited, result) = trigger("ba", TriggerOutcome::Skip);
        assert_eq!(visited, vc(&["r", "ba", "bb", "bb_la", "bb_lb"]));
        assert_walk_result(result, Ok(Walk::Continue));

        let (visited, result) = trigger("r", TriggerOutcome::Skip);
        assert_eq!(visited, vc(&["r"]));
        assert_walk_result(result, Ok(Walk::Continue));

        let (visited, result) = trigger("ba", TriggerOutcome::Handle);
        assert_eq!(visited, vc(&["r", "ba"]));
        assert_walk_result(result, Ok(Walk::Handle(())));

        let (visited, result) = trigger("ba_la", TriggerOutcome::Handle);
        assert_eq!(visited, vc(&["r", "ba", "ba_la"]));
        assert_walk_result(result, Ok(Walk::Handle(())));

        let (visited, result) = trigger("ba_la", TriggerOutcome::NoResult);
        assert_eq!(visited, vc(&["r", "ba", "ba_la"]));
        assert_walk_result(result, Err(Error::Internal("no result".into())));

        let (visited, result) = trigger("r", TriggerOutcome::NoResult);
        assert_eq!(visited, vc(&["r"]));
        assert_walk_result(result, Err(Error::Internal("no result".into())));

        Ok(())
    }

    #[test]
    fn test_postorder() -> Result<()> {
        fn trigger(name: &str, outcome: TriggerOutcome) -> (Vec<String>, Result<Walk<()>>) {
            let mut canopy = Canopy::new();
            let (root, _ba, _bb, _ba_la, _ba_lb, _bb_la, _bb_lb) = build_tree(&mut canopy).unwrap();
            let mut v = Vec::new();
            let res = canopy.with_root_view(|context| {
                postorder(context, root, &mut |id| -> Result<Walk<()>> {
                    let name_str = node_name(context, root, id);
                    v.push(name_str.clone());
                    if name_str == name {
                        outcome_result(outcome)
                    } else {
                        Ok(Walk::Continue)
                    }
                })
            });
            (v, res)
        }

        let (visited, result) = trigger("ba_la", TriggerOutcome::Skip);
        assert_eq!(visited, vc(&["ba_la", "ba", "r"]));
        assert_walk_result(result, Ok(Walk::Skip));

        let (visited, result) = trigger("ba_lb", TriggerOutcome::Skip);
        assert_eq!(visited, vc(&["ba_la", "ba_lb", "ba", "r"]));
        assert_walk_result(result, Ok(Walk::Skip));

        let (visited, result) = trigger("r", TriggerOutcome::Skip);
        assert_eq!(
            visited,
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"])
        );
        assert_walk_result(result, Ok(Walk::Skip));

        let (visited, result) = trigger("bb", TriggerOutcome::Skip);
        assert_eq!(
            visited,
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb", "r"])
        );
        assert_walk_result(result, Ok(Walk::Skip));

        let (visited, result) = trigger("ba", TriggerOutcome::Skip);
        assert_eq!(visited, vc(&["ba_la", "ba_lb", "ba", "r"]));
        assert_walk_result(result, Ok(Walk::Skip));

        let (visited, result) = trigger("ba_la", TriggerOutcome::Handle);
        assert_eq!(visited, vc(&["ba_la"]));
        assert_walk_result(result, Ok(Walk::Handle(())));

        let (visited, result) = trigger("bb", TriggerOutcome::Handle);
        assert_eq!(
            visited,
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"])
        );
        assert_walk_result(result, Ok(Walk::Handle(())));

        let (visited, result) = trigger("ba_la", TriggerOutcome::NoResult);
        assert_eq!(visited, vc(&["ba_la"]));
        assert_walk_result(result, Err(Error::Internal("no result".into())));

        let (visited, result) = trigger("bb", TriggerOutcome::NoResult);
        assert_eq!(
            visited,
            vc(&["ba_la", "ba_lb", "ba", "bb_la", "bb_lb", "bb"])
        );
        assert_walk_result(result, Err(Error::Internal("no result".into())));

        Ok(())
    }

    fn locate_name(canopy: &Canopy, root: NodeId, point: Point) -> Result<Option<String>> {
        canopy.with_root_view(|context| {
            context
                .locate(root, point)
                .map(|node| node.map(|node| node_name(context, root, node)))
        })
    }

    fn focused_name(canopy: &Canopy) -> Option<String> {
        canopy.with_root_view(|context| {
            let root = context.root_id();
            context
                .focused_leaf(root)
                .map(|node| node_name(context, root, node))
        })
    }

    fn focus_first(canopy: &mut Canopy, root: NodeId) -> Result<()> {
        canopy.with_root_context(|context| context.focus_first(FocusScope::Node(root)).map(|_| ()))
    }

    fn focus_dir(canopy: &mut Canopy, root: NodeId, direction: Direction) -> Result<()> {
        canopy.with_root_context(|context| {
            context
                .focus_dir(FocusScope::Node(root), direction)
                .map(|_| ())
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

    #[test]
    fn test_focus_dir_navigation() -> Result<()> {
        let mut canopy = Canopy::new();
        let grid = Grid::install(&mut canopy, 1, 2)?;

        focus_first(&mut canopy, grid.root)?;
        assert_eq!(focused_name(&canopy), Some("cell_0_0".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Right)?;
        assert_eq!(focused_name(&canopy), Some("cell_1_0".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Down)?;
        assert_eq!(focused_name(&canopy), Some("cell_1_1".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Left)?;
        assert_eq!(focused_name(&canopy), Some("cell_0_1".to_string()));

        focus_dir(&mut canopy, grid.root, Direction::Up)?;
        assert_eq!(focused_name(&canopy), Some("cell_0_0".to_string()));

        Ok(())
    }
}
