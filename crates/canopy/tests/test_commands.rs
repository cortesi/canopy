//! Integration tests for command dispatch.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use canopy::{
        Canopy, Context, ReadContext, Widget, command,
        commands::{
            ArgValue, CommandDispatchKind, CommandError, CommandNode, CommandResolution,
            CommandResolver, dispatch,
        },
        derive_commands,
        error::Result,
        render::Render,
    };

    // Test helper to record command calls
    thread_local! {
        static STATE_PATH: RefCell<Vec<String>> = const { RefCell::new(vec![]) };
    }

    fn state_path() -> Vec<String> {
        STATE_PATH.with(|s| s.borrow().clone())
    }

    fn reset_state() {
        STATE_PATH.with(|s| s.borrow_mut().clear());
    }

    struct TestLeaf;

    #[derive_commands]
    impl TestLeaf {
        #[command]
        fn c_leaf(&self, _c: &mut dyn Context) {
            STATE_PATH.with(|s| {
                s.borrow_mut().push(format!("{}.c_leaf()", self.name()));
            });
        }
    }

    impl Widget for TestLeaf {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }
    }

    struct TestBranch;

    #[derive_commands]
    impl TestBranch {
        #[command]
        fn c_branch(&self, _c: &mut dyn Context) {
            STATE_PATH.with(|s| {
                s.borrow_mut().push(format!("{}.c_branch()", self.name()));
            });
        }
    }

    impl Widget for TestBranch {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_command_dispatch() -> Result<()> {
        reset_state();

        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        let leaf_id = canopy.core_mut().create_detached(TestLeaf);
        let branch_id = canopy.core_mut().create_detached(TestBranch);
        canopy.core_mut().set_children(branch_id, vec![leaf_id])?;
        let root_id = canopy.root_id();
        canopy.core_mut().set_children(root_id, vec![branch_id])?;

        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();
        let result = dispatch(canopy.core_mut(), branch_id, &inv)?;

        assert_eq!(result, ArgValue::Null);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        Ok(())
    }

    #[test]
    fn duplicate_command_ids_are_deduplicated() -> Result<()> {
        reset_state();

        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        canopy.add_commands::<TestLeaf>()?;

        let leaf_id = canopy.core_mut().create_detached(TestLeaf);
        let branch_id = canopy.core_mut().create_detached(TestBranch);
        canopy.core_mut().set_children(branch_id, vec![leaf_id])?;
        let root_id = canopy.root_id();
        canopy.core_mut().set_children(root_id, vec![branch_id])?;

        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();
        let result = dispatch(canopy.core_mut(), branch_id, &inv)?;
        assert_eq!(result, ArgValue::Null);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        Ok(())
    }

    #[test]
    fn node_dispatch_reports_no_target() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();

        let root_id = canopy.core().root_id();
        let err = dispatch(canopy.core_mut(), root_id, &inv).unwrap_err();
        let owner_name = match TestLeaf::cmd_c_leaf().dispatch {
            CommandDispatchKind::Node { owner } => owner,
            CommandDispatchKind::Free => "free",
        };

        assert!(matches!(
            err,
            CommandError::NoTarget { ref id, ref owner }
                if id == inv.id.0 && owner == owner_name
        ));

        Ok(())
    }

    #[test]
    fn command_resolver_matches_dispatch_targets() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        canopy.add_commands::<TestBranch>()?;
        let first_leaf = canopy.core_mut().create_detached(TestLeaf);
        let second_leaf = canopy.core_mut().create_detached(TestLeaf);
        let branch_id = canopy.core_mut().create_detached(TestBranch);
        canopy
            .core_mut()
            .set_children(branch_id, vec![first_leaf, second_leaf])?;
        let root_id = canopy.root_id();
        canopy.core_mut().set_children(root_id, vec![branch_id])?;

        let resolver = CommandResolver::new(canopy.core(), branch_id);
        assert_eq!(
            resolver.resolve(TestLeaf::cmd_c_leaf()),
            Some(CommandResolution::Subtree { target: first_leaf })
        );

        let resolver = CommandResolver::new(canopy.core(), first_leaf);
        assert_eq!(
            resolver.resolve(TestBranch::cmd_c_branch()),
            Some(CommandResolution::Ancestor { target: branch_id })
        );

        let availability = resolver.availability();
        let branch_availability = availability
            .iter()
            .find(|availability| availability.spec.id == TestBranch::cmd_c_branch().id)
            .expect("branch command availability");
        assert_eq!(
            branch_availability.resolution,
            Some(CommandResolution::Ancestor { target: branch_id })
        );

        Ok(())
    }

    #[test]
    fn test_load_commands() -> Result<()> {
        struct Foo {
            a_triggered: bool,
            b_triggered: bool,
        }

        #[derive_commands]
        impl Foo {
            #[command]
            /// This is a comment.
            /// Multiline too!
            fn a(&mut self, _core: &mut dyn Context) -> Result<()> {
                self.a_triggered = true;
                Ok(())
            }

            #[command]
            fn b(&mut self, _core: &mut dyn Context) -> Result<()> {
                self.b_triggered = true;
                Ok(())
            }
        }

        impl Widget for Foo {
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
                Ok(())
            }
        }

        let commands = Foo::commands();
        assert_eq!(commands.len(), 2);

        // Check that commands are properly loaded
        assert!(commands.iter().any(|c| c.name == "a"));
        assert!(commands.iter().any(|c| c.name == "b"));

        let cmd_a = Foo::cmd_a();
        assert_eq!(cmd_a.id.0, "foo::a");
        assert_eq!(cmd_a.signature(), "foo::a() -> ()");

        Ok(())
    }
}
