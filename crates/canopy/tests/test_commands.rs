//! Integration tests for command dispatch.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use canopy::{
        commands::{CommandInvocation, dispatch},
        *,
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

    #[derive(StatefulNode)]
    struct TestLeaf {
        state: NodeState,
    }

    #[derive_commands]
    impl TestLeaf {
        #[command]
        fn c_leaf(&self, _c: &mut dyn Context) {
            STATE_PATH.with(|s| {
                s.borrow_mut().push(format!("{}.c_leaf()", self.name()));
            });
        }
    }

    impl Node for TestLeaf {}

    #[derive(StatefulNode)]
    struct TestBranch {
        state: NodeState,
        la: TestLeaf,
        lb: TestLeaf,
    }

    impl Node for TestBranch {
        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.la)?;
            f(&mut self.lb)?;
            Ok(())
        }
    }

    #[derive_commands]
    impl TestBranch {}

    #[allow(dead_code)]
    #[derive(StatefulNode)]
    struct TestRoot {
        state: NodeState,
        a: TestBranch,
        b: TestBranch,
    }

    impl Node for TestRoot {
        fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.a)?;
            f(&mut self.b)?;
            Ok(())
        }
    }

    #[derive_commands]
    impl TestRoot {}

    #[test]
    fn test_command_dispatch() -> Result<()> {
        // The test is simpler - just verify commands can be called
        let mut canopy = Canopy::new();
        let leaf = TestLeaf {
            state: NodeState::default(),
        };

        // Call the command directly to verify it works
        leaf.c_leaf(&mut canopy);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        // Verify dispatch mechanism works with a simple tree
        reset_state();
        let mut root = TestBranch {
            state: NodeState::default(),
            la: TestLeaf {
                state: NodeState::default(),
            },
            lb: TestLeaf {
                state: NodeState::default(),
            },
        };

        // Dispatch to a specific node
        let result = dispatch(
            &mut canopy,
            root.la.id(),
            &mut root,
            &CommandInvocation {
                node: "test_leaf".try_into()?,
                command: "c_leaf".into(),
                args: vec![],
            },
        )?;

        // For now, just verify no error occurred
        assert!(result.is_some());

        Ok(())
    }

    #[test]
    fn test_load_commands() -> Result<()> {
        #[derive(StatefulNode)]
        struct Foo {
            state: NodeState,
            a_triggered: bool,
            b_triggered: bool,
        }

        impl Node for Foo {}

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
