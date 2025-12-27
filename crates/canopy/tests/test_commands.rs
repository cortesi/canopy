//! Integration tests for command dispatch.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use canopy::{
        Context, Core, ViewContext, command,
        commands::{CommandInvocation, CommandNode, dispatch},
        derive_commands,
        error::Result,
        render::Render,
        widget::Widget,
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
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    struct TestBranch;

    #[derive_commands]
    impl TestBranch {}

    impl Widget for TestBranch {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_command_dispatch() -> Result<()> {
        reset_state();

        let mut core = Core::new();
        let leaf_id = core.add(TestLeaf);
        let branch_id = core.add(TestBranch);
        core.set_children(branch_id, vec![leaf_id])?;
        core.set_children(core.root, vec![branch_id])?;

        let result = dispatch(
            &mut core,
            branch_id,
            &CommandInvocation {
                node: "test_leaf".try_into()?,
                command: "c_leaf".into(),
                args: vec![],
            },
        )?;

        assert!(result.is_some());
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

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
            fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
                Ok(())
            }
        }

        let commands = Foo::commands();
        assert_eq!(commands.len(), 2);

        // Check that commands are properly loaded
        assert!(commands.iter().any(|c| c.command == "a"));
        assert!(commands.iter().any(|c| c.command == "b"));

        // Check that the documentation is preserved
        let cmd_a = commands.iter().find(|c| c.command == "a").unwrap();
        assert!(cmd_a.docs.contains("This is a comment"));
        assert!(cmd_a.docs.contains("Multiline too!"));

        Ok(())
    }
}
