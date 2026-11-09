//! Mount hook integration tests.

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc, time::Duration};

    use canopy::{
        CanopyBuilder, Context, ContextExt, NodeId, NodeName, TypedId, Widget,
        error::{Error, Result},
        geom::Size,
        runtime::TurnInput,
        testing::harness::Harness,
    };

    struct MountProbe {
        mount_calls: usize,
        mounted_id: Option<NodeId>,
        mounted_root: Option<NodeId>,
        child_id: Option<TypedId<ChildProbe>>,
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

    impl Widget for MountProbe {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            self.mount_calls += 1;
            self.mounted_id = Some(ctx.node_id());
            self.mounted_root = Some(ctx.root_id());
            let child = ctx.add_child(ctx.node_id(), ChildProbe::new())?;
            self.child_id = Some(child);
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("mount_probe")
        }
    }

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

    impl Widget for ChildProbe {
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
        assert_eq!(mounted_root, Some(harness.canopy.root_id()));
        let child_id = child_id.expect("child id missing");
        let child_id = NodeId::from(child_id);

        let (child_calls, child_mounted_id) = harness
            .with_widget(child_id, |probe: &mut ChildProbe| {
                (probe.mount_calls, probe.mounted_id)
            });

        assert_eq!(child_calls, 1);
        assert_eq!(child_mounted_id, Some(child_id));

        Ok(())
    }

    /// Widget whose mount fails, counting the polls it receives.
    struct FailedMount {
        polls: Rc<Cell<usize>>,
    }

    impl Widget for FailedMount {
        fn on_mount(&mut self, _ctx: &mut dyn Context) -> Result<()> {
            Err(Error::app("mount failed"))
        }

        fn poll(&mut self, _ctx: &mut dyn Context) -> Result<Option<Duration>> {
            self.polls.set(self.polls.get() + 1);
            Ok(None)
        }
    }

    /// Widget that builds a child whose mount fails.
    struct FailingParent {
        polls: Rc<Cell<usize>>,
    }

    impl Widget for FailingParent {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let polls = Rc::clone(&self.polls);
            ctx.add_child(ctx.node_id(), FailedMount { polls })?;
            Ok(())
        }
    }

    #[test]
    fn a_node_whose_mount_fails_is_never_polled() -> Result<()> {
        let polls = Rc::new(Cell::new(0));
        let mut canopy = CanopyBuilder::new().build()?;
        canopy.set_screen_size(Size::new(10, 2))?;
        let mounted = canopy.replace_root(FailingParent {
            polls: Rc::clone(&polls),
        });
        assert!(mounted.is_err(), "the child's failed mount fails the edit");
        canopy.turn(TurnInput::Prepare)?;
        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(polls.get(), 0);
        Ok(())
    }
}
