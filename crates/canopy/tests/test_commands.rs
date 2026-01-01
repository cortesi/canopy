//! Integration tests for command dispatch.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use canopy::{
        Canopy, Context, ViewContext, Widget, command,
        commands::{ArgValue, CommandNode, dispatch},
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

        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>();
        let leaf_id = canopy.core.create_detached(TestLeaf);
        let branch_id = canopy.core.create_detached(TestBranch);
        canopy.core.set_children(branch_id, vec![leaf_id])?;
        canopy
            .core
            .set_children(canopy.core.root_id(), vec![branch_id])?;

        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();
        let result = dispatch(&mut canopy.core, branch_id, &inv)?;

        assert_eq!(result, ArgValue::Null);
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
        assert!(commands.iter().any(|c| c.name == "a"));
        assert!(commands.iter().any(|c| c.name == "b"));

        let cmd_a = Foo::cmd_a();
        assert_eq!(cmd_a.id.0, "foo::a");
        assert_eq!(cmd_a.signature(), "foo::a() -> ()");

        Ok(())
    }
}
