//! Tree traversal and hit-testing integration tests.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, CanopyBuilder, Context, ContextExt, NodeId, NodeName, ViewContext, Widget,
        error::{Error, Result},
        path::Path,
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

    #[derive(Default)]
    struct MountCounter {
        attempts: usize,
    }

    impl Widget for MountCounter {
        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            self.attempts += 1;
            Err(Error::Invalid("mount rejected".into()))
        }
    }

    #[test]
    fn failed_mount_restores_structure_but_retains_widget_mutation() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.with_root_context(|ctx| {
            let child = ctx.create_detached(MountCounter::default())?;
            for expected_attempts in 1..=2 {
                let error = ctx.edit_structure(&mut |ctx| ctx.attach(ctx.node_id(), child.into()));
                assert!(matches!(error, Err(Error::Invalid(_))));
                assert!(ctx.children_of(ctx.node_id()).is_empty());
                assert!(ctx.type_id_of(child.into()).is_some());
                ctx.with_widget_mut(child, |widget: &mut MountCounter, _| {
                    assert_eq!(widget.attempts, expected_attempts);
                    Ok(())
                })?;
            }
            Ok(())
        })
    }

    #[test]
    fn nested_structural_rollback_retains_widget_mutation() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.with_root_context(|ctx| {
            let child = ctx.create_detached(TreeWidget::new("original"))?;
            let root = ctx.node_id();
            ctx.edit_structure(&mut |ctx| {
                ctx.attach(root, child.into())?;
                let error = ctx.edit_structure(&mut |ctx| {
                    ctx.with_widget_mut(child, |widget: &mut TreeWidget, _| {
                        widget.name = "changed".into();
                        Ok(())
                    })?;
                    ctx.detach(child.into())?;
                    Err(Error::Invalid("nested edit rejected".into()))
                });
                assert!(matches!(error, Err(Error::Invalid(_))));
                assert_eq!(ctx.children_of(ctx.node_id()), vec![child.into()]);
                // The node's captured metadata and the widget's own state
                // differ.
                assert_eq!(node_name(ctx, root, child.into()), "original");
                ctx.with_widget_mut(child, |widget: &mut TreeWidget, _| {
                    assert_eq!(widget.name, "changed");
                    Ok(())
                })?;
                Ok(())
            })?;
            assert_eq!(ctx.children_of(ctx.node_id()), vec![child.into()]);
            Ok(())
        })
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
            context.set_children(root, vec![ba, bb])?;
            context.set_children(ba, vec![ba_la, ba_lb])?;
            context.set_children(bb, vec![bb_la, bb_lb])?;
            Ok((root, ba, bb, ba_la, ba_lb, bb_la, bb_lb))
        })
    }

    #[test]
    fn test_node_path() -> Result<()> {
        let mut canopy = CanopyBuilder::new().build()?;
        let (root, _ba, _bb, ba_la, _ba_lb, _bb_la, _bb_lb) = build_tree(&mut canopy)?;

        canopy.with_root_view(|context| {
            assert_eq!(context.path_of(root, root), Path::new(["r"]));
            assert_eq!(
                context.path_of(root, ba_la),
                Path::new(["r", "ba", "ba_la"])
            );
        });

        Ok(())
    }

    fn node_name(context: &dyn ViewContext, root: NodeId, node: NodeId) -> String {
        context
            .path_of(root, node)
            .pop()
            .expect("node path should contain a name")
    }
}
