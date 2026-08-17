//! Command dispatch, argument, and error integration tests.

#[cfg(test)]
mod tests {
    use std::{any::Any, cell::RefCell, collections::BTreeMap};

    use canopy::{
        Canopy, CommandArg, CommandEnum, Context, ViewContext, Widget, command,
        commands::{
            ArgValue, CommandArgs, CommandDispatchKind, CommandError, CommandInvocation,
            CommandNode, CommandResolution, FromArgValue, SerdeArg, ToArgValue,
        },
        derive_commands,
        error::Result,
        event::Event,
        render::Render,
        testing::dummyctx::DummyContext,
    };
    use serde::{Deserialize, Serialize};

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
    impl TestBranch {
        #[command]
        fn c_branch(&self, _c: &mut dyn Context) {
            STATE_PATH.with(|s| {
                s.borrow_mut().push(format!("{}.c_branch()", self.name()));
            });
        }
    }

    impl Widget for TestBranch {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_command_dispatch() -> Result<()> {
        reset_state();

        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        let branch_id = canopy.with_root_context(|context| {
            let leaf_id = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children_of(branch_id.into(), vec![leaf_id.into()])?;
            context.set_children(vec![branch_id.into()])?;
            Ok(branch_id)
        })?;

        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();
        let result =
            canopy.with_context(branch_id, |context| Ok(context.dispatch_command(&inv)))??;

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

        let branch_id = canopy.with_root_context(|context| {
            let leaf_id = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children_of(branch_id.into(), vec![leaf_id.into()])?;
            context.set_children(vec![branch_id.into()])?;
            Ok(branch_id)
        })?;

        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();
        let result =
            canopy.with_context(branch_id, |context| Ok(context.dispatch_command(&inv)))??;
        assert_eq!(result, ArgValue::Null);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        Ok(())
    }

    #[test]
    fn node_dispatch_reports_no_target() -> Result<()> {
        let mut canopy = Canopy::new();
        canopy.add_commands::<TestLeaf>()?;
        let inv = TestLeaf::cmd_c_leaf().call_with(()).invocation();

        let err = canopy
            .with_root_context(|context| Ok(context.dispatch_command(&inv)))?
            .unwrap_err();
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
        let (first_leaf, branch_id) = canopy.with_root_context(|context| {
            let first_leaf = context.create_detached(TestLeaf)?;
            let second_leaf = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children_of(
                branch_id.into(),
                vec![first_leaf.into(), second_leaf.into()],
            )?;
            context.set_children(vec![branch_id.into()])?;
            Ok((first_leaf, branch_id))
        })?;

        let availability = canopy.command_availability_from_node(branch_id.into());
        let leaf_availability = availability
            .iter()
            .find(|availability| availability.spec.id == TestLeaf::cmd_c_leaf().id)
            .expect("leaf command availability");
        assert_eq!(
            leaf_availability.resolution,
            Some(CommandResolution::Subtree {
                target: first_leaf.into()
            })
        );

        let availability = canopy.command_availability_from_node(first_leaf.into());
        let branch_availability = availability
            .iter()
            .find(|availability| availability.spec.id == TestBranch::cmd_c_branch().id)
            .expect("branch command availability");
        assert_eq!(
            branch_availability.resolution,
            Some(CommandResolution::Ancestor {
                target: branch_id.into()
            })
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
        assert!(cmd_a.params.is_empty());

        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CommandArg)]
    struct Inner {
        count: i32,
        label: String,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CommandArg)]
    struct Outer {
        name: String,
        inner: Inner,
        optional: Option<bool>,
        tags: Vec<String>,
        map: BTreeMap<String, usize>,
    }

    #[derive(Debug, Clone, PartialEq, CommandEnum)]
    enum Mode {
        Fast,
        Slow,
    }

    #[test]
    fn command_arg_round_trip_nested() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 1);
        map.insert("b".to_string(), 2);
        let value = Outer {
            name: "outer".to_string(),
            inner: Inner {
                count: 42,
                label: "inner".to_string(),
            },
            optional: Some(true),
            tags: vec!["x".to_string(), "y".to_string()],
            map,
        };

        let encoded = SerdeArg(value.clone()).try_to_arg_value().unwrap();
        let decoded = Outer::from_arg_value(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn command_enum_round_trip() {
        let value = Mode::Fast;
        let encoded = value.to_arg_value();
        assert_eq!(encoded, ArgValue::String("Fast".to_string()));
        let decoded = Mode::from_arg_value(&ArgValue::String("slow".to_string())).unwrap();
        assert_eq!(decoded, Mode::Slow);
    }

    #[test]
    fn command_enum_unknown_variant_errors() {
        let err = Mode::from_arg_value(&ArgValue::String("turbo".to_string())).unwrap_err();
        assert!(matches!(err, CommandError::Conversion { .. }));
    }

    #[test]
    fn u32_encodes_as_uint() {
        let value = (123u32).to_arg_value();
        assert!(matches!(value, ArgValue::UInt(_)));
        let back = u32::from_arg_value(&value).expect("u32 round-trip from ArgValue::UInt");
        assert_eq!(back, 123);
    }

    struct Tester {
        scroll: usize,
        hits: usize,
        last_event: Option<Event>,
    }

    #[derive_commands]
    impl Tester {
        fn new() -> Self {
            Self {
                scroll: 0,
                hits: 0,
                last_event: None,
            }
        }

        #[command]
        fn set_scroll(&mut self, _ctx: &mut dyn Context, scroll_count: usize) {
            self.scroll = scroll_count;
        }

        #[command]
        fn needs_event(&mut self, event: Event) {
            self.last_event = Some(event);
            self.hits += 1;
        }
    }

    #[test]
    fn positional_arity_mismatch() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = Tester::cmd_set_scroll().call_with(()).invocation();
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::ArityMismatch {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn type_mismatch_reports_param() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Positional(vec![ArgValue::String("bad".to_string())]),
        };
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::TypeMismatch { ref param, expected, ref got }
                if param == "scroll_count" && expected == "usize" && got == "String"
        ));
    }

    #[test]
    fn unknown_named_args_error() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let mut map = BTreeMap::new();
        map.insert("unknown".to_string(), ArgValue::Int(1));
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Named(map),
        };
        let err =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::UnknownNamedArg { ref name, .. } if name == "unknown"
        ));
    }

    #[test]
    fn normalized_named_args_bind() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let mut map = BTreeMap::new();
        map.insert("Scroll-Count".to_string(), ArgValue::Int(3));
        let inv = CommandInvocation {
            id: Tester::cmd_set_scroll().id,
            args: CommandArgs::Named(map),
        };
        let out =
            (Tester::cmd_set_scroll().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap();

        assert_eq!(out, ArgValue::Null);
        assert_eq!(tester.scroll, 3);
    }

    #[test]
    fn missing_injected_value_errors() {
        let mut tester = Tester::new();
        let mut ctx = DummyContext::default();
        let inv = Tester::cmd_needs_event().call_with(()).invocation();
        let err =
            (Tester::cmd_needs_event().invoke)(Some(&mut tester as &mut dyn Any), &mut ctx, &inv)
                .unwrap_err();

        assert!(matches!(
            err,
            CommandError::MissingInjected { ref param, expected }
                if param == "event" && expected == "Event"
        ));
        assert_eq!(tester.hits, 0);
    }
}
