//! Command dispatch, argument, and error integration tests.

#[cfg(test)]
mod tests {
    use std::{any::Any, cell::RefCell, collections::BTreeMap};

    use canopy::{
        CanopyBuilder, CommandArg, CommandEnum, Context, ContextExt, ViewContext, Widget,
        commands::{
            ArgValue, CommandArgs, CommandError, CommandResolution, CommandStatus, CommandTarget,
            FromArgValue, SerdeArg, ToArgValue,
        },
        derive_commands,
        error::{Error, Result},
        event::Event,
    };
    use serde::{Deserialize, Serialize};

    /// Run `f` against the root context of an empty application.
    fn with_ctx<R>(f: impl FnOnce(&mut dyn Context) -> R) -> R {
        CanopyBuilder::new()
            .build()
            .expect("an empty application builds")
            .with_root_context(|ctx| Ok(f(ctx)))
            .expect("root context is available")
    }

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

    impl Widget for TestLeaf {}

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

    impl Widget for TestBranch {}

    #[test]
    fn test_command_dispatch() -> Result<()> {
        reset_state();

        let mut canopy = CanopyBuilder::new()
            .configure(|setup| setup.add_commands::<TestLeaf>())
            .build()?;
        let branch_id = canopy.with_root_context(|context| {
            let leaf_id = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children(branch_id.into(), vec![leaf_id.into()])?;
            context.set_children(context.node_id(), vec![branch_id.into()])?;
            Ok(branch_id)
        })?;

        let call = TestLeaf::call_c_leaf();
        let result = canopy.with_context(branch_id, |context| Ok(context.dispatch(&call)))??;

        assert_eq!(result, ArgValue::Null);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        Ok(())
    }

    #[test]
    fn duplicate_command_ids_are_deduplicated() -> Result<()> {
        reset_state();

        let mut canopy = CanopyBuilder::new()
            .configure(|setup| {
                setup.add_commands::<TestLeaf>()?;
                setup.add_commands::<TestLeaf>()
            })
            .build()?;

        let branch_id = canopy.with_root_context(|context| {
            let leaf_id = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children(branch_id.into(), vec![leaf_id.into()])?;
            context.set_children(context.node_id(), vec![branch_id.into()])?;
            Ok(branch_id)
        })?;

        let call = TestLeaf::call_c_leaf();
        let result = canopy.with_context(branch_id, |context| Ok(context.dispatch(&call)))??;
        assert_eq!(result, ArgValue::Null);
        assert_eq!(state_path(), vec!["test_leaf.c_leaf()"]);

        Ok(())
    }

    #[test]
    fn node_dispatch_reports_no_target() -> Result<()> {
        let mut canopy = CanopyBuilder::new()
            .configure(|setup| setup.add_commands::<TestLeaf>())
            .build()?;
        let call = TestLeaf::call_c_leaf();

        let err = canopy
            .with_root_context(|context| Ok(context.dispatch(&call)))?
            .unwrap_err();
        let owner_name = TestLeaf::spec_c_leaf().owner;

        assert!(matches!(
            err,
            CommandError::NoTarget { ref id, ref owner }
                if id == call.id.0 && owner == owner_name
        ));

        Ok(())
    }

    #[test]
    fn command_resolver_matches_dispatch_targets() -> Result<()> {
        let mut canopy = CanopyBuilder::new()
            .configure(|setup| {
                setup.add_commands::<TestLeaf>()?;
                setup.add_commands::<TestBranch>()
            })
            .build()?;
        let (first_leaf, branch_id) = canopy.with_root_context(|context| {
            let first_leaf = context.create_detached(TestLeaf)?;
            let second_leaf = context.create_detached(TestLeaf)?;
            let branch_id = context.create_detached(TestBranch)?;
            context.set_children(
                branch_id.into(),
                vec![first_leaf.into(), second_leaf.into()],
            )?;
            context.set_children(context.node_id(), vec![branch_id.into()])?;
            Ok((first_leaf, branch_id))
        })?;

        let availability = canopy.command_availability(CommandTarget::From(branch_id.into()))?;
        let leaf_availability = availability
            .iter()
            .find(|availability| availability.spec.id == TestLeaf::spec_c_leaf().id)
            .expect("leaf command availability");
        assert_eq!(
            leaf_availability.resolution,
            Some(CommandResolution::Subtree {
                target: first_leaf.into()
            })
        );

        let availability = canopy.command_availability(CommandTarget::From(first_leaf.into()))?;
        let branch_availability = availability
            .iter()
            .find(|availability| availability.spec.id == TestBranch::spec_c_branch().id)
            .expect("branch command availability");
        assert_eq!(
            branch_availability.resolution,
            Some(CommandResolution::Ancestor {
                target: branch_id.into()
            })
        );

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
        ExtraFast,
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
        assert_eq!(
            Mode::Fast.to_arg_value(),
            ArgValue::String("fast".to_string())
        );
        assert_eq!(
            Mode::ExtraFast.to_arg_value(),
            ArgValue::String("extra_fast".to_string())
        );
        for (name, expected) in [
            ("slow", Mode::Slow),
            ("Slow", Mode::Slow),
            ("extra_fast", Mode::ExtraFast),
            ("EXTRA_FAST", Mode::ExtraFast),
        ] {
            let decoded = Mode::from_arg_value(&ArgValue::String(name.to_string())).unwrap();
            assert_eq!(decoded, expected);
        }
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
        let args = CommandArgs::default();
        let err = with_ctx(|ctx| {
            (Tester::spec_set_scroll().invoke)(&mut tester as &mut dyn Any, ctx, &args)
        })
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
        let args = CommandArgs::Positional(vec![ArgValue::String("bad".to_string())]);
        let err = with_ctx(|ctx| {
            (Tester::spec_set_scroll().invoke)(&mut tester as &mut dyn Any, ctx, &args)
        })
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
        let mut map = BTreeMap::new();
        map.insert("unknown".to_string(), ArgValue::Int(1));
        let args = CommandArgs::Named(map);
        let err = with_ctx(|ctx| {
            (Tester::spec_set_scroll().invoke)(&mut tester as &mut dyn Any, ctx, &args)
        })
        .unwrap_err();

        assert!(matches!(
            err,
            CommandError::UnknownNamedArg { ref name, .. } if name == "unknown"
        ));
    }

    #[test]
    fn normalized_named_args_bind() {
        let mut tester = Tester::new();
        let mut map = BTreeMap::new();
        map.insert("Scroll-Count".to_string(), ArgValue::Int(3));
        let args = CommandArgs::Named(map);
        let out = with_ctx(|ctx| {
            (Tester::spec_set_scroll().invoke)(&mut tester as &mut dyn Any, ctx, &args)
        })
        .unwrap();

        assert_eq!(out, ArgValue::Null);
        assert_eq!(tester.scroll, 3);
    }

    #[test]
    fn missing_injected_value_errors() {
        let mut tester = Tester::new();
        let args = CommandArgs::default();
        let err = with_ctx(|ctx| {
            (Tester::spec_needs_event().invoke)(&mut tester as &mut dyn Any, ctx, &args)
        })
        .unwrap_err();

        assert!(matches!(
            err,
            CommandError::MissingInjected { ref param, expected }
                if param == "event" && expected == "Event"
        ));
        assert_eq!(tester.hits, 0);
    }
    struct TargetCounter {
        count: i64,
        enabled: bool,
    }

    #[derive_commands]
    impl TargetCounter {
        fn can_increment(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
            Ok(if self.enabled {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("counter paused".into())
            })
        }

        #[command(enabled = "can_increment")]
        fn increment(&mut self, amount: i64) {
            self.count += amount;
        }
    }

    impl Widget for TargetCounter {
        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }
    }

    #[test]
    fn exact_from_and_focus_targets_share_discovery_and_recheck_status() -> Result<()> {
        let mut app = CanopyBuilder::new()
            .configure(|setup| setup.add_commands::<TargetCounter>())
            .build()?;
        let (first, second) = app.with_root_context(|ctx| {
            let first = ctx.add_child(
                ctx.node_id(),
                TargetCounter {
                    count: 0,
                    enabled: true,
                },
            )?;
            let second = ctx.add_child(
                ctx.node_id(),
                TargetCounter {
                    count: 0,
                    enabled: true,
                },
            )?;
            ctx.set_focus(second.into())?;
            Ok((first, second))
        })?;
        let call = TargetCounter::call_increment(2);
        for (target, expected) in [
            (CommandTarget::From(app.root_id()), first),
            (CommandTarget::Focus, second),
            (CommandTarget::Exact(second.into()), second),
        ] {
            let availability = app.command_availability(target)?;
            let command = availability
                .iter()
                .find(|entry| entry.spec.id == call.id)
                .unwrap();
            assert_eq!(
                command.resolution.map(CommandResolution::target),
                Some(expected.into())
            );
            assert_eq!(command.status, Some(CommandStatus::Enabled));
            let call = call.clone().with_target(target);
            app.with_root_context(|ctx| Ok(ctx.dispatch(&call)?))?;
        }
        // A call without a target resolves from the dispatching node.
        app.with_context(second, |ctx| Ok(ctx.dispatch(&call)?))?;
        app.with_root_context(|ctx| {
            ctx.with_widget_mut(first, |counter: &mut TargetCounter, _| {
                assert_eq!(counter.count, 2);
                Ok(())
            })?;
            ctx.with_widget_mut(second, |counter: &mut TargetCounter, _| {
                assert_eq!(counter.count, 6);
                counter.enabled = false;
                Ok(())
            })
        })?;
        let exact = call
            .clone()
            .with_target(CommandTarget::Exact(second.into()));
        app.with_root_view(|ctx| {
            assert_eq!(
                ctx.command_status(&exact)?,
                CommandStatus::Disabled("counter paused".into())
            );
            Ok::<_, Error>(())
        })?;
        let err = app
            .with_root_context(|ctx| Ok(ctx.dispatch(&exact)))?
            .unwrap_err();
        assert!(matches!(err, CommandError::Disabled { reason, .. } if reason == "counter paused"));
        let err = app
            .with_root_context(|ctx| Ok(ctx.dispatch_exact(ctx.root_id(), &call)))?
            .unwrap_err();
        assert!(matches!(err, CommandError::WrongOwner { .. }));
        app.with_root_context(|ctx| ctx.remove_subtree(second.into()))?;
        let err = app
            .with_root_context(|ctx| Ok(ctx.dispatch(&exact)))?
            .unwrap_err();
        assert!(matches!(err, CommandError::InvalidNode { .. }));
        Ok(())
    }
}
