//! Luau scripting framework and command integration tests.

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs, path::Path, rc::Rc};

    use canopy::{
        Canopy, CanopyBuilder, CommandArg, Context, ContextExt, EventOutcome, NodeId, Register,
        Setup, ViewContext, Widget,
        commands::ArgValue,
        derive_commands,
        error::{Error, Result, ScriptErrorKind},
        geom::{Line, Size},
        input::{
            BindingAction, BindingOptions, BindingPhase, BindingTier, Event, FrameworkBindingGroup,
            key::Key, mouse,
        },
        layout::Layout,
        render::Render,
        runtime::TurnInput,
        script::{EvalRequest, ScriptOrigin, ScriptTrust},
        testing::{backend::TestRender, harness::Harness},
    };
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    struct ApiLeaf {
        value: i32,
    }

    #[derive_commands]
    impl ApiLeaf {
        fn new() -> Self {
            Self { value: 0 }
        }

        #[command]
        fn set(&mut self, value: i32) {
            self.value = value;
        }

        #[command]
        fn get(&self) -> i32 {
            self.value
        }
    }

    impl Widget for ApiLeaf {
        fn render(&mut self, frame: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            frame.text("default", Line::new(0, 0, 8), &self.value.to_string())?;
            Ok(())
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
            match event {
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::Down,
                    button: mouse::Button::Left,
                    ..
                }) => {
                    self.value = 21;
                    Ok(EventOutcome::Handle)
                }
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::ScrollDown,
                    ..
                }) => {
                    self.value = 22;
                    Ok(EventOutcome::Handle)
                }
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::Drag,
                    ..
                }) => {
                    self.value = 23;
                    Ok(EventOutcome::Handle)
                }
                Event::Mouse(mouse::MouseEvent {
                    action: mouse::Action::ScrollRight,
                    ..
                }) => {
                    self.value = 24;
                    Ok(EventOutcome::Handle)
                }
                _ => Ok(EventOutcome::Ignore),
            }
        }

        fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
            true
        }
    }

    impl Register for ApiLeaf {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()
        }
    }

    struct ApiRoot;

    impl Widget for ApiRoot {
        fn layout(&self) -> Layout {
            Layout::row()
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let left = ctx.add_child(ctx.node_id(), ApiLeaf::new())?;
            let right = ctx.add_child(ctx.node_id(), ApiLeaf::new())?;
            ctx.set_layout_override(left.into(), Layout::fill().into())?;
            ctx.set_layout_override(right.into(), Layout::fill().into())?;
            ctx.set_focus(left.into())?;
            Ok(())
        }
    }

    impl Register for ApiRoot {
        fn register(setup: &mut Setup) -> Result<()> {
            ApiLeaf::register(setup)
        }
    }

    fn leaf_ids(harness: &Harness) -> Vec<NodeId> {
        harness
            .find_nodes("api_root/api_leaf")
            .expect("valid path filter")
    }

    fn leaf_values(harness: &mut Harness) -> Vec<i32> {
        leaf_ids(harness)
            .into_iter()
            .map(|node| harness.with_widget::<ApiLeaf, _>(node, |leaf| leaf.value))
            .collect()
    }

    /// Return a temporary directory that removes itself when the test ends.
    fn test_dir() -> TempDir {
        tempfile::tempdir().expect("create test directory")
    }

    fn write_script(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create script parent");
        }
        fs::write(path, source).expect("write script");
    }

    /// Start an application that registers [`ApiLeaf`] and mounts one leaf
    /// under the root.
    fn leaf_builder() -> CanopyBuilder {
        CanopyBuilder::new()
            .configure(ApiLeaf::register)
            .assemble(|canopy| {
                canopy.with_root_context(|context| {
                    let leaf = context.create_detached(ApiLeaf::new())?;
                    context.set_children(context.node_id(), vec![leaf.into()])
                })
            })
    }

    fn raw_canopy_with_leaf() -> Result<Canopy> {
        leaf_builder().build()
    }

    /// Return how many journal entries came from startup sources.
    fn startup_runs(canopy: &Canopy) -> usize {
        canopy
            .script_journal()
            .iter()
            .filter(|entry| matches!(entry.origin, ScriptOrigin::Startup(_)))
            .count()
    }

    #[test]
    fn framework_functions_are_available_from_luau() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;

        harness.script(
            r#"
            local root = canopy.root()
            local root_info = canopy.node_info(root)
            canopy.assert(root_info.name == "api_root", "root node should expose the widget name")
            canopy.assert(root_info.parent == nil, "root should not have a parent")

            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.assert(#leaves == 2, "expected two focusable leaves")
            local first = leaves[1]
            local second = leaves[2]

            canopy.assert(
                canopy.find_node("api_root/api_leaf") == first,
                "find_node should return the first matching leaf"
            )
            canopy.assert(canopy.node_info(first).parent == root, "leaf parent should be the root")

            local children = root_info.children
            canopy.assert(#children == 2, "root should expose both children")
            canopy.assert(children[1] == first, "first child should match the first leaf")
            canopy.assert(children[2] == second, "second child should match the second leaf")

            canopy.assert(canopy.focused() ~= nil, "a leaf should be focused after mount")
            canopy.set_focus(first)
            canopy.assert(canopy.focused() == first, "focus should move to the first leaf")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should hit the first leaf")

            canopy.call_from(second, "api_leaf::set", 9)
            canopy.assert(canopy.call_from(second, "api_leaf::get") == 9, "call_from should target a node")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should remain on the first leaf")

            canopy.set_focus(second)
            canopy.assert(canopy.focused() == second, "focus should move to the second leaf")

            canopy.move_focus("prev")
            canopy.assert(canopy.focused() == first, "focus_prev should move back to the first leaf")
            canopy.move_focus("next")
            canopy.assert(canopy.focused() == second, "focus_next should move to the second leaf")
            canopy.set_focus(first)
            canopy.move_focus("right")
            canopy.assert(canopy.focused() == second, "focus_dir right should move to the second leaf")

            canopy.send_click(1, 1)
            canopy.assert(
                canopy.call_from(first, "api_leaf::get") == 21,
                "send_click should dispatch a left click to the target node"
            )
            canopy.send_scroll("down", 1, 1)
            canopy.assert(
                canopy.call_from(first, "api_leaf::get") == 22,
                "send_scroll should dispatch a scroll event to the target node"
            )
            canopy.send_scroll("right", 1, 1)
            canopy.assert(
                canopy.call_from(first, "api_leaf::get") == 24,
                "send_scroll should dispatch horizontal wheel steps"
            )
            canopy.send_drag(1, 1, 2, 1)
            canopy.assert(
                canopy.call_from(first, "api_leaf::get") == 23,
                "send_drag should press, drag, and release"
            )
        "#,
        )?;

        assert_eq!(leaf_values(&mut harness), vec![23, 9]);
        Ok(())
    }

    #[test]
    fn luau_bindings_replace_unbind_and_clear_correctly() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])

            canopy.bind("x", { description = "old" }, function() api_leaf.set(3) end)
            canopy.bind("x", { description = "new" }, function() api_leaf.set(7) end)

            local transient = canopy.bind("u", { description = "Transient binding" }, function() api_leaf.set(99) end)
            canopy.unbind(transient)
        "#,
        )?;

        harness.script(r#"canopy.send_key("x")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.script(r#"canopy.send_key("u")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("z", { description = "Set value" }, function() api_leaf.set(15) end)
            canopy.unbind_key("z")
        "#,
        )?;
        harness.script(r#"canopy.send_key("z")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("c", { description = "Set value" }, function() api_leaf.set(21) end)
            canopy.clear_bindings()
        "#,
        )?;
        harness.script(r#"canopy.send_key("c")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        Ok(())
    }

    #[test]
    fn command_values_bind_keys_and_mouse_and_report_their_arguments() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])
            local id = canopy.bind("x", { description = "Set three" }, command.api_leaf.set(3))
            canopy.keymap({ { mouse = "ScrollUp", description = "Set five", action = command.api_leaf.set(5) } })
            local found = false
            for _, binding in canopy.bindings() do
                if binding.id == id then
                    found = binding.action == "command"
                        and binding.command == "api_leaf::set"
                        and binding.arguments[1] == 3
                        and binding.phase == "after_widget"
                end
            end
            canopy.assert(found, "a command binding reports its command and arguments")
            canopy.assert(
                tostring(command.api_leaf.set(3)) == "api_leaf::set(3)",
                "a CommandCall prints its command and arguments"
            )
            canopy.assert(command.api_leaf.set(3) == command.api_leaf.set(3), "equal calls compare equal")
            "#,
        )?;
        harness.script(r#"canopy.send_key("x")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![3, 0]);
        harness.script(r#"canopy.send_scroll("up", 1, 1)"#)?;
        assert_eq!(leaf_values(&mut harness), vec![5, 0]);
        Ok(())
    }

    #[test]
    fn command_constructors_reject_bad_arguments_before_any_binding_installs() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        let runtime_type = harness
            .canopy
            .eval_script(
                r#"
                local bad: any = "oops"
                canopy.bind("x", { description = "Bad" }, command.api_leaf.set(bad))
                "#,
            )
            .expect_err("a string argument for a number parameter must fail");
        assert!(
            runtime_type.to_string().contains("type mismatch"),
            "{runtime_type}"
        );
        let runtime_arity = harness
            .canopy
            .eval_script(
                r#"
                local set: any = command.api_leaf.set
                canopy.bind("x", { description = "Bad" }, set())
                "#,
            )
            .expect_err("a missing argument must fail");
        assert!(
            runtime_arity.to_string().contains("arity mismatch"),
            "{runtime_arity}"
        );
        for source in [
            // The typechecker rejects a wrong literal argument.
            r#"canopy.bind("x", { description = "Bad" }, command.api_leaf.set("1"))"#,
            // An owner function call runs at once and returns no action.
            r#"canopy.bind("x", { description = "Bad" }, api_leaf.get())"#,
            // bind_command no longer exists.
            r#"canopy.bind_command("x", { description = "Old" }, "api_leaf::set", 1)"#,
            // An action must be a CommandCall or a function.
            r#"local bad: any = "api_leaf::set"
               canopy.bind("x", { description = "Bad" }, bad)"#,
        ] {
            harness
                .canopy
                .eval_script(source)
                .expect_err("invalid binding action should fail");
        }
        harness.script(r#"canopy.send_key("x")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![0, 0]);
        Ok(())
    }

    #[test]
    fn owner_named_command_fails_api_finalization() -> Result<()> {
        struct Command;

        #[derive_commands]
        impl Command {
            #[command]
            fn run(&self) {}
        }

        impl Widget for Command {}

        let Err(error) = CanopyBuilder::new()
            .configure(|setup| setup.add_commands::<Command>())
            .build()
        else {
            panic!("an owner named command collides with the command global");
        };
        assert!(error.to_string().contains("reserved"), "{error}");
        Ok(())
    }

    #[test]
    fn keymap_installs_every_entry_in_order() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])
            local ids = canopy.keymap({
                {
                    key = { "a", "b" },
                    mouse = "ScrollUp",
                    description = "Set one",
                    action = command.api_leaf.set(1),
                },
                { key = "c", description = "Set two", action = function() api_leaf.set(2) end },
            })
            canopy.assert(#ids == 4, "one binding per key and mouse spec")
            for index = 2, #ids do
                canopy.assert(ids[index] > ids[index - 1], "ids follow entry order")
            end
            local sources = {}
            local inputs = {}
            for _, binding in canopy.bindings() do
                for _, id in ids do
                    if binding.id == id then
                        table.insert(sources, binding.source)
                        table.insert(inputs, binding.input)
                    end
                end
            end
            canopy.assert(#sources == 4, "every binding is reported")
            for _, source in sources do
                canopy.assert(source == sources[1], "every binding records the keymap call site")
            end
            canopy.assert(inputs[3] == "ScrollUp", "key bindings come before mouse bindings")
            canopy.assert(#canopy.keymap({}) == 0, "an empty keymap installs nothing")
            "#,
        )?;
        harness.script(r#"canopy.send_key("c")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![2, 0]);
        harness.script(r#"canopy.send_key("b")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![1, 0]);
        harness.script(r#"canopy.send_key("c")"#)?;
        harness.script(r#"canopy.send_scroll("up", 1, 1)"#)?;
        assert_eq!(leaf_values(&mut harness), vec![1, 0]);

        // A later keymap replaces an earlier binding with the same selector.
        harness.canopy.eval_script(
            r#"
            canopy.keymap({
                { key = "a", description = "Set nine", action = command.api_leaf.set(9) },
            })
            "#,
        )?;
        harness.script(r#"canopy.send_key("a")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![9, 0]);

        // A keymap puts key and mouse entries in the same phase.
        harness.canopy.eval_script(
            r#"
            canopy.keymap({
                phase = "before_widget",
                { key = "e", description = "Set three", action = command.api_leaf.set(3) },
                { mouse = "ScrollDown", description = "Set four", action = command.api_leaf.set(4) },
            })
            for _, binding in canopy.bindings() do
                if binding.input == "e" or binding.input == "ScrollDown" then
                    canopy.assert(
                        binding.phase == "before_widget",
                        "every entry takes the keymap's phase"
                    )
                end
            end
            "#,
        )?;
        harness.script(r#"canopy.send_scroll("down", 1, 1)"#)?;
        assert_eq!(leaf_values(&mut harness), vec![4, 0]);
        Ok(())
    }

    #[test]
    fn keymap_rejects_invalid_input_and_installs_nothing() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])
            "#,
        )?;
        for (source, expected) in [
            (
                r#"canopy.keymap({ mdoe = "preview", { key = "x", description = "Set", action = command.api_leaf.set(1) } })"#,
                "",
            ),
            (
                r#"canopy.keymap({ { key = "x", mosue = "ScrollUp", description = "Set", action = command.api_leaf.set(1) } })"#,
                "keymap entry 1 has an unknown field `mosue`",
            ),
            (
                r#"canopy.keymap({ { description = "Set", action = command.api_leaf.set(1) } })"#,
                "keymap entry 1 has neither `key` nor `mouse`",
            ),
            (
                r#"canopy.keymap({ { key = {}, description = "Set", action = command.api_leaf.set(1) } })"#,
                "is an empty array",
            ),
            (
                r#"canopy.keymap({ { key = "Ctrl+", description = "Set", action = command.api_leaf.set(1) } })"#,
                "invalid key spec",
            ),
            (
                r#"canopy.keymap({
                    { key = "x", description = "A", action = command.api_leaf.set(1) },
                    { key = "x", description = "B", action = command.api_leaf.set(2) },
                })"#,
                "keymap entry 2 binds `x` more than once",
            ),
            (
                r#"canopy.keymap({ { key = "x", action = command.api_leaf.set(1) } })"#,
                "",
            ),
            (
                r#"canopy.keymap({ tier = "global", { key = "x", description = "Set", action = command.api_leaf.set(1) } })"#,
                "anchored",
            ),
            (
                r#"local bad: any = "api_leaf::set"
                   canopy.keymap({ { key = "x", description = "Set", action = bad } })"#,
                "must be dotted",
            ),
            (
                r#"local bad: any = "oops"
                   canopy.keymap({ { key = "x", description = "Set", action = command.api_leaf.set(bad) } })"#,
                "type mismatch",
            ),
            (
                r#"local entries: any = { { key = "x", description = "Set", action = command.api_leaf.set(1) } }
                   entries[3] = entries[1]
                   canopy.keymap(entries)"#,
                "dense array",
            ),
        ] {
            let error = harness
                .canopy
                .eval_script(source)
                .expect_err("invalid keymap should fail");
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
        harness.script(r#"canopy.send_key("x")"#)?;
        harness.script(r#"canopy.send_scroll("up", 1, 1)"#)?;
        assert_eq!(leaf_values(&mut harness), vec![0, 0]);
        Ok(())
    }

    #[test]
    fn binding_contract_rejects_missing_or_obsolete_options() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        for source in [
            r#"canopy.bind("a", function() end)"#,
            r#"canopy.bind("a", {}, function() end)"#,
            r#"canopy.bind("a", { description = " " }, function() end)"#,
            r#"canopy.bind("a", { description = "A", mode = "insert", tier = "global" }, function() end)"#,
            r#"canopy.bind("a", { description = "A", tier = "other" }, function() end)"#,
            r#"canopy.bind_with("a", {}, function() end)"#,
            r#"canopy.bind_mouse("LeftDown", { description = "Removed" }, function() end)"#,
        ] {
            harness
                .canopy
                .eval_script(source)
                .expect_err("invalid binding contract should fail");
        }
        Ok(())
    }

    #[test]
    fn script_registry_reports_framework_records_but_cannot_remove_them() -> Result<()> {
        let bound = Rc::new(Cell::new(None));
        let record = Rc::clone(&bound);
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .configure(move |setup| {
                let group = FrameworkBindingGroup::new("test.framework");
                let id = setup.bind(
                    Key::parse_spec("F1")?,
                    BindingOptions {
                        path: Some("/api_root/**/".parse()?),
                        tier: BindingTier::Framework(group),
                        description: "Framework action".to_string(),
                        source: None,
                        phase: Some(BindingPhase::AfterWidget),
                    },
                    BindingAction::Command(ApiLeaf::call_get()),
                )?;
                record.set(Some(id));
                Ok(())
            })
            .size(20, 5)
            .build()?;
        let id = bound.get().expect("the framework binding was installed");
        harness.render()?;

        harness.canopy.eval_script(&format!(
            r#"
            local found = false
            for _, binding in canopy.bindings() do
                if binding.id == {} then
                    found = binding.tier == "framework"
                        and binding.group == "test.framework"
                        and binding.action == "command"
                        and binding.description == "Framework action"
                end
            end
            canopy.assert(found, "framework binding metadata should be complete")
            "#,
            id.as_u64()
        ))?;
        let error = harness
            .canopy
            .eval_script(&format!("canopy.unbind({})", id.as_u64()))
            .expect_err("scripts must not remove framework records");
        assert!(error.to_string().contains("framework-owned"));
        Ok(())
    }

    #[test]
    fn stored_callback_prints_use_fresh_call_options() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness
            .canopy
            .eval_script(r#"canopy.bind("p", { description = "Print callback" }, function() print("callback print") end)"#)?;
        let _ = harness.canopy.take_script_logs();

        harness.key('p')?;
        assert_eq!(
            harness.canopy.take_script_logs(),
            vec!["callback print".to_string()]
        );
        Ok(())
    }

    #[test]
    fn script_find_rejects_invalid_path_filters() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;

        let err = harness
            .script(r#"canopy.find_node("api-root")"#)
            .expect_err("invalid path filter should fail");
        assert!(err.to_string().contains("api-root"));

        Ok(())
    }

    #[test]
    fn luau_nested_callbacks_can_unbind_and_dispatch() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])

            local nested = 0
            nested = canopy.bind("n", { description = "Nested callback" }, function()
                canopy.unbind(nested)
                canopy.call_from(leaves[2], "api_leaf::set", 41)
            end)

            local outer = 0
            outer = canopy.bind("o", { description = "Outer callback" }, function()
                canopy.send_key("n")
                api_leaf.set(17)
                canopy.unbind(outer)
            end)
        "#,
        )?;

        harness.script(r#"canopy.send_key("o")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![17, 41]);

        harness.script(
            r#"
            canopy.send_key("o")
            canopy.send_key("n")
        "#,
        )?;
        assert_eq!(leaf_values(&mut harness), vec![17, 41]);

        Ok(())
    }

    #[test]
    fn luau_can_switch_modes() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;

        harness.canopy.eval_script(
            r#"
            canopy.set_mode("insert")
            canopy.assert(canopy.mode() == "insert", "mode should switch")
            canopy.push_mode("palette")
            canopy.assert(canopy.mode() == "palette", "push should activate top mode")
            canopy.assert(canopy.pop_mode() == "insert", "pop should restore previous mode")
            canopy.assert(canopy.pop_mode() == "", "pop should return to default mode")
        "#,
        )?;

        assert_eq!(harness.canopy.mode(), "");
        Ok(())
    }

    #[test]
    fn luau_observation_helpers_expose_runtime_state() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r##"
            canopy.send_key("x")

            canopy.prepare()
            local frame = canopy.snapshot()
            assert(frame)
            local cells = frame.cells
            canopy.assert(#cells > 0, "screen cells should include rows")
            canopy.assert(cells[1][1].fg:sub(1, 1) == "#", "cell fg should be RGB text")

            local region = canopy.screen_text({x = 0, y = 0, w = 8, h = 1})
            canopy.assert(type(region) == "string", "screen region should be text")

            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.assert(canopy.screen_text(leaves[1]):find("0") ~= nil, "a node target should crop text")

            local trace = canopy.route_trace()
            canopy.assert(#trace > 0, "route trace should record the injected key")

            local dump = canopy.diagnostic_dump(leaves[1])
            canopy.assert(dump:find("node tree") ~= nil, "diagnostic dump should include the tree")

            local help = canopy.available_bindings()
            canopy.assert(help.focus ~= nil, "help snapshot should include focus")
            canopy.assert(type(help.bindings) == "table", "help snapshot should include bindings")

            local api = canopy.api()
            canopy.assert(api:find("declare canopy") ~= nil, "api text should be script-visible")
        "##,
        )?;

        assert_eq!(harness.canopy.script_journal().len(), 1);
        harness.canopy.eval_script(
            r#"
            local journal = canopy.script_journal()
            canopy.assert(#journal == 1, "previous eval should be journaled")
            canopy.assert(journal[1].ok, "previous eval should have succeeded")
            canopy.assert(#journal[1].assertions > 0, "journal should preserve assertions")
        "#,
        )?;
        assert_eq!(harness.canopy.script_journal().len(), 2);

        Ok(())
    }

    #[test]
    fn script_journal_is_bounded_with_monotonic_ids() -> Result<()> {
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.set_script_journal_limit(2);
        canopy.eval_script("api_leaf.set(1)")?;
        canopy.eval_script("api_leaf.set(2)")?;
        canopy.eval_script("api_leaf.set(3)")?;

        let journal = canopy.script_journal();
        assert_eq!(journal.len(), 2);
        assert_eq!(journal[0].id, 2);
        assert_eq!(journal[1].id, 3);
        Ok(())
    }

    #[test]
    fn nested_evaluations_keep_outer_diagnostics_and_journal_deltas() -> Result<()> {
        let mut canopy = leaf_builder()
            .configure(|setup| {
                setup.register_default_bindings("api_leaf", r#"canopy.log("from bindings")"#)
            })
            .build()?;
        canopy.eval_script(
            r#"
            canopy.log("outer before")
            api_leaf.default_bindings()
            canopy.log("outer after")
        "#,
        )?;

        let journal = canopy.script_journal();
        assert_eq!(journal.len(), 2);
        let nested = &journal[0];
        assert_eq!(nested.origin.to_string(), "default-bindings:api_leaf");
        assert_eq!(nested.logs, vec!["from bindings".to_string()]);
        let outer = &journal[1];
        assert_eq!(outer.origin.to_string(), "eval");
        assert_eq!(
            outer.logs,
            vec![
                "outer before".to_string(),
                "from bindings".to_string(),
                "outer after".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn startup_scripts_layer_app_user_and_project_modules() -> Result<()> {
        let dir = test_dir();
        let root = dir.path();
        let user_root = root.join("user");
        let project_root = root.join("work/.canopy");
        write_script(
            &user_root.join("keymap.luau"),
            r#"
            local M = {}
            function M.apply()
                api_leaf.set(api_leaf.get() + 2)
            end
            return M
        "#,
        );
        write_script(
            &user_root.join("keymap.d.luau"),
            r#"
            declare module: {
                apply: () -> (),
            }
        "#,
        );
        write_script(
            &user_root.join("init.luau"),
            r#"
            local keymap = require("@user/keymap")

            function setup()
                keymap.apply()
            end
        "#,
        );
        write_script(
            &project_root.join("project.luau"),
            r#"
            local M = {}
            function M.apply()
                api_leaf.set(api_leaf.get() + 30)
            end
            return M
        "#,
        );
        write_script(
            &project_root.join("project.d.luau"),
            r#"
            declare module: {
                apply: () -> (),
            }
        "#,
        );
        write_script(
            &project_root.join("init.luau"),
            r#"
            local project = require("@project/project")

            function setup()
                project.apply()
            end
        "#,
        );

        let mut canopy = leaf_builder()
            .user_script_root(user_root, ScriptTrust::TrustedLocal)
            .project_script_root(project_root, ScriptTrust::TrustedLocal)
            .configure(|setup| {
                setup.register_startup_script(
                    "app",
                    r#"
            function setup()
                api_leaf.set(1)
            end
        "#,
                )
            })
            .build()?;

        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(startup_runs(&canopy), 3);
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(33)
        );
        canopy.turn(TurnInput::Prepare)?;
        assert_eq!(startup_runs(&canopy), 3, "startup runs once");

        Ok(())
    }

    #[test]
    fn startup_scripts_require_setup_global() -> Result<()> {
        let Err(error) = leaf_builder()
            .configure(|setup| setup.register_startup_script("app", "api_leaf.set(1)"))
            .build()
        else {
            panic!("startup script without setup should fail typechecking");
        };
        assert!(
            error
                .to_string()
                .contains("startup/app:0:0: Required global 'setup'"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn startup_failure_releases_registered_callbacks() -> Result<()> {
        let mut canopy = leaf_builder()
            .configure(|setup| {
                setup.register_startup_script(
                    "failing",
                    r#"
            function setup()
                canopy.bind("x", { description = "Set value" }, function() api_leaf.set(99) end)
                error("startup failed")
            end
        "#,
                )
            })
            .build()?;

        assert!(
            canopy.turn(TurnInput::Prepare).is_err(),
            "startup execution should fail"
        );
        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(0)
        );
        Ok(())
    }

    #[test]
    fn a_failed_startup_keeps_earlier_scripts_and_restores_prior_registrations() -> Result<()> {
        let mut canopy = leaf_builder()
            .configure(|setup| {
                setup.register_startup_script(
                    "first",
                    r#"
            function setup()
                api_leaf.set(1)
                canopy.bind("x", { description = "Set value" }, function() api_leaf.set(7) end)
            end
        "#,
                )?;
                setup.register_startup_script(
                    "second",
                    r#"
            function setup()
                api_leaf.set(api_leaf.get() + 10)
                canopy.bind("x", { description = "Set value" }, function() api_leaf.set(99) end)
                canopy.on_start(function() api_leaf.set(88) end)
                error("second failed")
            end
        "#,
                )
            })
            .build()?;

        assert!(
            canopy.turn(TurnInput::Prepare).is_err(),
            "second startup should fail"
        );
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(11),
            "native effects of the failed script are not rolled back"
        );

        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(7),
            "the failed script's binding is rolled back"
        );

        let mut render = TestRender::new();
        canopy.set_screen_size(Size::new(10, 1))?;
        canopy.render(&mut render)?;
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(7),
            "the failed script's start hook never runs"
        );
        assert_eq!(startup_runs(&canopy), 2, "startup is attempted once");
        Ok(())
    }

    #[test]
    fn script_module_declarations_must_conform() -> Result<()> {
        let dir = test_dir();
        let root = dir.path();
        let project_root = root.join("work/.canopy");
        write_script(
            &project_root.join("settings.luau"),
            r#"
            return { value = "wrong" }
        "#,
        );
        write_script(
            &project_root.join("settings.d.luau"),
            r#"
            declare module: {
                value: number,
            }
        "#,
        );

        let build = || {
            leaf_builder()
                .project_script_root(project_root.clone(), ScriptTrust::TrustedLocal)
                .build()
        };
        let Err(err) = build() else {
            panic!("mismatched declaration should fail finalization");
        };
        assert!(err.to_string().contains("settings.d.luau"));

        write_script(
            &project_root.join("settings.d.luau"),
            r#"
            declare module: {
                value: string,
            }
        "#,
        );
        assert!(build()?.script_api().is_ok());

        Ok(())
    }

    #[test]
    fn config_loads_named_files_with_relative_requires() -> Result<()> {
        let dir = test_dir();
        let root = dir.path();
        let project_root = root.join("work/.canopy");
        write_script(
            &project_root.join("lib.luau"),
            r#"
            return { value = 44 }
        "#,
        );
        let config = project_root.join("main.luau");
        write_script(
            &config,
            r#"
            local lib = require("./lib")
            canopy.bind("x", { description = "Set value" }, function() api_leaf.set(lib.value) end)
        "#,
        );

        // Config runs before assembly, so it binds rather than calling the
        // leaf directly.
        let mut canopy = leaf_builder()
            .project_script_root(project_root.clone(), ScriptTrust::TrustedLocal)
            .script_file(config)
            .build()?;

        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(44)
        );

        write_script(&project_root.join("lib.luau"), "return { value = 45 }");
        assert!(
            canopy
                .invalidate_script_modules(Some("@project"))?
                .is_some()
        );
        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script("return api_leaf.get()")?,
            ArgValue::Int(44),
            "invalidation clears the bindings the config installed"
        );
        assert_eq!(
            canopy.eval_script(r#"return require("@project/lib").value"#)?,
            ArgValue::Int(45),
            "invalidation reloads the module source"
        );

        Ok(())
    }

    /// Payload used to prove structural command argument declarations.
    #[derive(Debug, Clone, Serialize, Deserialize, CommandArg)]
    struct Payload {
        /// Count carried through serde conversion.
        count: usize,
    }

    /// Self-referential payload used to prove declaration recursion terminates.
    #[derive(Debug, Clone, Serialize, Deserialize, CommandArg)]
    struct TreePayload {
        /// Node label.
        label: String,
        /// Child subtrees.
        children: Vec<Self>,
    }

    struct ScriptTarget {
        value: usize,
        payload_value: usize,
        last_payload: Option<Payload>,
        tree_label: Option<String>,
    }

    #[derive_commands]
    impl ScriptTarget {
        fn new() -> Self {
            Self {
                value: 0,
                payload_value: 0,
                last_payload: None,
                tree_label: None,
            }
        }

        #[command]
        fn set(&mut self, _ctx: &mut dyn Context, count: usize) {
            self.value = count;
        }

        #[command]
        fn set_optional(&mut self, count: Option<usize>) {
            self.value = count.unwrap_or(99);
        }

        #[command]
        fn set_payload(&mut self, _ctx: &mut dyn Context, payload: Payload) {
            let Payload { count } = payload;
            self.payload_value = count;
            self.last_payload = Some(payload);
        }

        #[command]
        fn set_tree(&mut self, tree: TreePayload) {
            self.value = tree.children.len();
            self.tree_label = Some(tree.label);
        }
    }

    impl Widget for ScriptTarget {}

    impl Register for ScriptTarget {
        fn register(setup: &mut Setup) -> Result<()> {
            setup.add_commands::<Self>()?;
            Ok(())
        }
    }

    #[test]
    fn script_helpers_dispatch_commands() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new())
            .register::<ScriptTarget>()
            .size(10, 1)
            .build()?;

        harness.script(r#"canopy.call_named("script_target::set", { count = 7 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 7);
        });

        harness.script(r#"script_target.set(12)"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 12);
        });

        harness.script(r#"canopy.call_named("script_target::set", { count = 13 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 13);
        });

        harness.script(r#"script_target.set(9)"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 9);
        });

        harness.script(r#"script_target.set(5)"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 5);
        });

        harness.script(r#"script_target.set_optional()"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 99);
        });

        harness.script(r#"canopy.call_named("script_target::set_optional", { count = 14 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 14);
        });

        harness.script(r#"script_target.set_payload({ count = 3 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.payload_value, 3);
        });

        harness.script(r#"script_target.set_payload({ count = 4 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.payload_value, 4);
        });

        let err = harness
            .script(r#"canopy.call_from(canopy.root(), "script_target::set", { foo = 11 })"#)
            .expect_err("a table for a number is a structured script error");
        let Error::ScriptStructured { kind, command, .. } = err else {
            panic!("expected structured script error, got {err:?}");
        };
        assert_eq!(kind, ScriptErrorKind::TypeMismatch);
        assert_eq!(command, None);

        Ok(())
    }

    #[test]
    fn command_discovery_reports_contract_and_availability() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new())
            .register::<ScriptTarget>()
            .size(10, 1)
            .build()?;

        let command = harness.canopy.eval_script(
            r#"
            local found: any = nil
            for _, command in ipairs(canopy.commands()) do
                if command.owner == "script_target" and command.name == "set" then
                    found = command
                end
            end
            return found
            "#,
        )?;
        let ArgValue::Map(command) = command else {
            panic!("command metadata is a record");
        };
        assert_eq!(
            command.get("ret"),
            Some(&ArgValue::String("()".to_string()))
        );
        assert_eq!(command.get("available"), Some(&ArgValue::Bool(true)));
        let Some(ArgValue::Map(target)) = command.get("target") else {
            panic!("command target should be an external node token: {command:?}");
        };
        assert_eq!(
            target.get("type"),
            Some(&ArgValue::String("NodeId".to_string()))
        );
        assert!(matches!(target.get("token"), Some(ArgValue::String(_))));

        let resolved = harness
            .canopy
            .eval_script(r#"return canopy.target("script_target") ~= nil"#)?;
        assert_eq!(resolved, ArgValue::Bool(true));

        let forged_node = harness.canopy.eval_script(
            r#"
            local forged: any = 1
            local ok, err = pcall(function()
                canopy.node_info(forged)
            end)
            local detail: any = err
            return { ok = ok, kind = detail.kind, expected = detail.expected }
            "#,
        )?;
        let ArgValue::Map(forged_node) = forged_node else {
            panic!("structured node error is a record");
        };
        assert_eq!(forged_node.get("ok"), Some(&ArgValue::Bool(false)));
        assert_eq!(
            forged_node.get("kind"),
            Some(&ArgValue::String("type_mismatch".to_string()))
        );
        assert_eq!(
            forged_node.get("expected"),
            Some(&ArgValue::String("NodeId".to_string()))
        );

        let error = harness.canopy.eval_script(
            r#"
            local ok, err = pcall(function()
                canopy.call_named("missing::command", {})
            end)
            local detail: any = err
            return { ok = ok, kind = detail.kind, command = detail.command }
            "#,
        )?;
        let ArgValue::Map(error) = error else {
            panic!("structured error is a record");
        };
        assert_eq!(error.get("ok"), Some(&ArgValue::Bool(false)));
        assert_eq!(
            error.get("kind"),
            Some(&ArgValue::String("unknown_command".to_string()))
        );
        assert_eq!(
            error.get("command"),
            Some(&ArgValue::String("missing::command".to_string()))
        );

        let payload_param = harness.canopy.eval_script(
            r#"
            for _, command in ipairs(canopy.commands()) do
                if command.name == "set_payload" then
                    return command.params[1]
                end
            end
            error("missing command")
            "#,
        )?;
        let ArgValue::Map(payload_param) = payload_param else {
            panic!("parameter metadata is a record");
        };
        assert_eq!(
            payload_param.get("luau_type"),
            Some(&ArgValue::String("Payload".to_string()))
        );

        Ok(())
    }

    #[test]
    fn script_diagnostics_capture_logs_and_assertions() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new())
            .register::<ScriptTarget>()
            .size(10, 1)
            .build()?;

        let outcome = harness.canopy.eval(EvalRequest::new(
            r#"canopy.log("hello"); canopy.assert(true, "ok"); return 7"#,
        ))?;
        assert_eq!(outcome.logs, vec!["hello"]);
        let assertions = outcome.assertions.clone();
        assert_eq!(outcome.into_result()?, ArgValue::Int(7));
        assert!(
            harness.canopy.take_script_logs().is_empty(),
            "evaluation output stays on its outcome"
        );
        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed);
        assert_eq!(assertions[0].message, "ok");

        Ok(())
    }

    #[test]
    fn on_start_hooks_run_during_preparation_after_geometry() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new())
            .register::<ScriptTarget>()
            .size(10, 1)
            .build()?;

        harness
            .canopy
            .eval_script("canopy.on_start(function() script_target.set(21) end)")?;
        // Synchronous eval drives its final preparation before returning.
        harness.render()?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 21);
        });

        harness.render()?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 21);
        });

        Ok(())
    }

    #[test]
    fn failing_on_start_hook_releases_drained_and_newly_queued_hooks() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new())
            .register::<ScriptTarget>()
            .size(10, 1)
            .build()?;
        harness
            .canopy
            .eval_script(
                r#"
            canopy.on_start(function() script_target.set(1) end)
            canopy.on_start(function()
                canopy.on_start(function() script_target.set(4) end)
                error("hook failed")
            end)
            canopy.on_start(function() script_target.set(3) end)
        "#,
            )
            .expect_err("second hook should fail during evaluation preparation");
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 1);
        });

        harness.render()?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 1);
        });
        Ok(())
    }

    #[test]
    fn recursive_command_arg_declarations_terminate() -> Result<()> {
        let canopy = CanopyBuilder::new()
            .configure(ScriptTarget::register)
            .build()?;

        let api = canopy.script_api()?;
        assert_eq!(api.matches("export type TreePayload").count(), 1);
        assert!(api.contains("children: {TreePayload}"));
        Ok(())
    }
    #[test]
    fn screen_regions_clamp_signed_dimensions() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        harness.render()?;
        harness.script(
            r#"
            for _, size in {{-1, 1}, {1, -1}, {0, 1}, {1, 0}} do
                canopy.assert(canopy.screen_text({x = 0, y = 0, w = size[1], h = size[2]}) == "")
            end
            local screen = canopy.screen_text()
            canopy.assert(canopy.screen_text({x = 0, y = 0, w = 20, h = 5}) == screen)
            canopy.assert(canopy.screen_text({x = 0, y = 0, w = 5000000000, h = 5000000000}) == screen)
            canopy.assert(canopy.screen_text({x = -1, y = -1, w = 21, h = 6}) == screen)
            canopy.assert(canopy.screen_text({x = 30, y = 30, w = 10, h = 10}) == "")
            canopy.assert(canopy.screen_text({x = 0, y = 0, w = 1, h = 1}) == "0")
        "#,
        )
    }

    #[test]
    fn node_info_declarations_match_runtime_records() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot)
            .register::<ApiRoot>()
            .size(20, 5)
            .build()?;
        let source = r#"
            local function visit(node: NodeId)
                local info = canopy.node_info(node)
                local name: string = info.name
                for _, child in info.children do
                    canopy.assert(canopy.node_info(child).parent == node)
                    visit(child)
                end
            end
            visit(canopy.root())
        "#;
        let checked = harness.canopy.check_script("node-info.luau", source)?;
        assert!(!checked.has_errors(), "{:?}", checked.diagnostics());
        harness.script(source)?;
        Ok(())
    }
}
