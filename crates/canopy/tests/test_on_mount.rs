//! Integration tests for widget mount hooks.

#[cfg(test)]
mod tests {
    use std::any::Any;

    use canopy::{
        Context, Core, Loader, NodeId, ViewContext,
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::Result,
        geom::Rect,
        render::Render,
        state::NodeName,
        testing::harness::Harness,
        widget::Widget,
    };

    struct MountProbe {
        mount_calls: usize,
        mounted_id: Option<NodeId>,
        mounted_root: Option<NodeId>,
        child_id: Option<NodeId>,
    }

    impl MountProbe {
        fn new() -> Self {
            Self {
                mount_calls: 0,
                mounted_id: None,
                mounted_root: None,
                child_id: None,
            }
        }
    }

    impl CommandNode for MountProbe {
        fn commands() -> Vec<CommandSpec> {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
        }
    }

    impl Widget for MountProbe {
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            self.mount_calls += 1;
            self.mounted_id = Some(ctx.node_id());
            self.mounted_root = Some(ctx.root_id());
            let child = ctx.add(Box::new(ChildProbe::new()));
            ctx.set_children(ctx.node_id(), vec![child])?;
            self.child_id = Some(child);
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("mount_probe")
        }
    }

    impl Loader for MountProbe {}

    struct ChildProbe {
        mount_calls: usize,
        mounted_id: Option<NodeId>,
    }

    impl ChildProbe {
        fn new() -> Self {
            Self {
                mount_calls: 0,
                mounted_id: None,
            }
        }
    }

    impl CommandNode for ChildProbe {
        fn commands() -> Vec<CommandSpec> {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
        }
    }

    impl Widget for ChildProbe {
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            self.mount_calls += 1;
            self.mounted_id = Some(ctx.node_id());
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("child_probe")
        }
    }

    fn mount_probe(core: &Core, node_id: NodeId) -> &MountProbe {
        let widget = core.nodes[node_id]
            .widget
            .as_ref()
            .expect("missing mount probe widget");
        let any = widget.as_ref() as &dyn Any;
        any.downcast_ref::<MountProbe>()
            .expect("mount probe type mismatch")
    }

    fn child_probe(core: &Core, node_id: NodeId) -> &ChildProbe {
        let widget = core.nodes[node_id]
            .widget
            .as_ref()
            .expect("missing child probe widget");
        let any = widget.as_ref() as &dyn Any;
        any.downcast_ref::<ChildProbe>()
            .expect("child probe type mismatch")
    }

    #[test]
    fn on_mount_runs_once_with_bound_context() -> Result<()> {
        let mut harness = Harness::builder(MountProbe::new()).size(10, 10).build()?;
        harness.render()?;
        harness.render()?;

        let child_id = {
            let probe = mount_probe(&harness.canopy.core, harness.root);
            assert_eq!(probe.mount_calls, 1);
            assert_eq!(probe.mounted_id, Some(harness.root));
            assert_eq!(probe.mounted_root, Some(harness.canopy.core.root));
            probe.child_id.expect("child id missing")
        };

        let child = child_probe(&harness.canopy.core, child_id);
        assert_eq!(child.mount_calls, 1);
        assert_eq!(child.mounted_id, Some(child_id));

        Ok(())
    }
}
