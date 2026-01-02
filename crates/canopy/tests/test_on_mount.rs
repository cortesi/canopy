//! Integration tests for widget mount hooks.

#[cfg(test)]
mod tests {
    use canopy::{
        Context, Loader, NodeId, ReadContext, Widget,
        commands::{CommandNode, CommandSpec},
        error::Result,
        render::Render,
        state::NodeName,
        testing::harness::Harness,
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
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for MountProbe {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            self.mount_calls += 1;
            self.mounted_id = Some(ctx.node_id());
            self.mounted_root = Some(ctx.root_id());
            let child = ctx.add_child(ChildProbe::new())?;
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
        fn commands() -> &'static [&'static CommandSpec] {
            &[]
        }
    }

    impl Widget for ChildProbe {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
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

    #[test]
    fn on_mount_runs_once_with_bound_context() -> Result<()> {
        let mut harness = Harness::builder(MountProbe::new()).size(10, 10).build()?;
        harness.render()?;
        harness.render()?;

        let (mount_calls, mounted_id, mounted_root, child_id) =
            harness.with_widget(harness.root, |probe: &mut MountProbe| {
                (
                    probe.mount_calls,
                    probe.mounted_id,
                    probe.mounted_root,
                    probe.child_id,
                )
            });

        assert_eq!(mount_calls, 1);
        assert_eq!(mounted_id, Some(harness.root));
        assert_eq!(mounted_root, Some(harness.canopy.core.root_id()));
        let child_id = child_id.expect("child id missing");

        let (child_calls, child_mounted_id) = harness
            .with_widget(child_id, |probe: &mut ChildProbe| {
                (probe.mount_calls, probe.mounted_id)
            });

        assert_eq!(child_calls, 1);
        assert_eq!(child_mounted_id, Some(child_id));

        Ok(())
    }
}
