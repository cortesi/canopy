//! Integration tests for the framework Luau API surface.

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use canopy::{
        Canopy, Context, EventOutcome, Loader, NodeId, ReadContext, ScriptApiState, Widget,
        command,
        commands::ArgValue,
        derive_commands,
        error::Result,
        event::{Event, mouse},
        geom::{Line, Size},
        layout::Layout,
        render::Render,
        testing::{backend::TestRender, harness::Harness},
    };

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
        fn render(&mut self, frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
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
                _ => Ok(EventOutcome::Ignore),
            }
        }

        fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
            true
        }
    }

    impl Loader for ApiLeaf {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()
        }
    }

    struct ApiRoot;

    impl Widget for ApiRoot {
        fn layout(&self) -> Layout {
            Layout::row()
        }

        fn render(&mut self, _frame: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let left = ctx.add_child(ApiLeaf::new())?;
            let right = ctx.add_child(ApiLeaf::new())?;
            ctx.set_layout_of(left, Layout::fill())?;
            ctx.set_layout_of(right, Layout::fill())?;
            ctx.set_focus(left.into())?;
            Ok(())
        }
    }

    impl Loader for ApiRoot {
        fn load(c: &mut Canopy) -> Result<()> {
            ApiLeaf::load(c)
        }
    }

    fn leaf_ids(harness: &Harness) -> Vec<NodeId> {
        harness.find_nodes("api_root/api_leaf")
    }

    fn leaf_values(harness: &mut Harness) -> Vec<i32> {
        leaf_ids(harness)
            .into_iter()
            .map(|node| harness.with_widget::<ApiLeaf, _>(node, |leaf| leaf.value))
            .collect()
    }

    fn test_dir(name: &str) -> PathBuf {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch");
        let dir = PathBuf::from("tmp").join(format!(
            "canopy-script-framework-{name}-{}-{}",
            process::id(),
            started.as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create test directory");
        dir
    }

    fn write_script(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create script parent");
        }
        fs::write(path, source).expect("write script");
    }

    fn raw_canopy_with_leaf() -> Result<Canopy> {
        let mut canopy = Canopy::new();
        ApiLeaf::load(&mut canopy)?;
        let leaf = canopy.create_detached(ApiLeaf::new());
        canopy.set_root_child(leaf)?;
        Ok(canopy)
    }

    #[test]
    fn framework_functions_are_available_from_luau() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.script(
            r#"
            local root = canopy.root()
            local root_info = canopy.node_info(root)
            canopy.assert(root_info.name == "api_root", "root node should expose the widget name")
            canopy.assert(canopy.parent(root) == nil, "root should not have a parent")

            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.assert(#leaves == 2, "expected two focusable leaves")
            local first = leaves[1]
            local second = leaves[2]

            canopy.assert(
                canopy.find_node("api_root/api_leaf") == first,
                "find_node should return the first matching leaf"
            )
            canopy.assert(canopy.parent(first) == root, "leaf parent should be the root")

            local children = canopy.children(root)
            canopy.assert(#children == 2, "root should expose both children")
            canopy.assert(children[1] == first, "first child should match the first leaf")
            canopy.assert(children[2] == second, "second child should match the second leaf")

            canopy.assert(canopy.focused() ~= nil, "a leaf should be focused after mount")
            canopy.set_focus(first)
            canopy.assert(canopy.focused() == first, "focus should move to the first leaf")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should hit the first leaf")

            canopy.cmd_on(second, "api_leaf::set", 9)
            canopy.assert(canopy.cmd_on(second, "api_leaf::get") == 9, "cmd_on should target a node")
            canopy.assert(api_leaf.get() == 0, "focused dispatch should remain on the first leaf")

            canopy.set_focus(second)
            canopy.assert(canopy.focused() == second, "focus should move to the second leaf")

            canopy.focus_prev()
            canopy.assert(canopy.focused() == first, "focus_prev should move back to the first leaf")
            canopy.focus_next()
            canopy.assert(canopy.focused() == second, "focus_next should move to the second leaf")
            canopy.set_focus(first)
            canopy.focus_dir("Right")
            canopy.assert(canopy.focused() == second, "focus_dir right should move to the second leaf")

            canopy.send_click(1, 1)
            canopy.assert(
                canopy.cmd_on(first, "api_leaf::get") == 21,
                "send_click should dispatch a left click to the target node"
            )
            canopy.send_scroll("Down", 1, 1)
            canopy.assert(
                canopy.cmd_on(first, "api_leaf::get") == 22,
                "send_scroll should dispatch a scroll event to the target node"
            )
        "#,
        )?;

        assert_eq!(leaf_values(&mut harness), vec![22, 9]);
        Ok(())
    }

    #[test]
    fn luau_bindings_replace_unbind_and_clear_correctly() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])

            canopy.bind_with("x", { desc = "old" }, function() api_leaf.set(3) end)
            canopy.bind_with("x", { desc = "new" }, function() api_leaf.set(7) end)

            local transient = canopy.bind("u", function() api_leaf.set(99) end)
            canopy.unbind(transient)
        "#,
        )?;

        harness.script(r#"canopy.send_key("x")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.script(r#"canopy.send_key("u")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("z", function() api_leaf.set(15) end)
            canopy.unbind_key("z")
        "#,
        )?;
        harness.script(r#"canopy.send_key("z")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        harness.canopy.eval_script(
            r#"
            canopy.bind("c", function() api_leaf.set(21) end)
            canopy.clear_bindings()
        "#,
        )?;
        harness.script(r#"canopy.send_key("c")"#)?;
        assert_eq!(leaf_values(&mut harness), vec![7, 0]);

        Ok(())
    }

    #[test]
    fn stored_callback_prints_use_fresh_call_options() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;
        harness
            .canopy
            .eval_script(r#"canopy.bind("p", function() print("callback print") end)"#)?;
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
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        let err = harness
            .script(r#"canopy.find_node("api-root")"#)
            .expect_err("invalid path filter should fail");
        assert!(err.to_string().contains("api-root"));

        Ok(())
    }

    #[test]
    fn luau_nested_callbacks_can_unbind_and_dispatch() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r#"
            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.set_focus(leaves[1])

            local nested = 0
            nested = canopy.bind("n", function()
                canopy.unbind(nested)
                canopy.cmd_on(leaves[2], "api_leaf::set", 41)
            end)

            local outer = 0
            outer = canopy.bind("o", function()
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
    fn luau_can_switch_input_modes() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;

        harness.canopy.eval_script(
            r#"
            canopy.set_mode("insert")
            canopy.assert(canopy.input_mode() == "insert", "mode should switch")
            canopy.push_mode("palette")
            canopy.assert(canopy.input_mode() == "palette", "push should activate top mode")
            canopy.assert(canopy.pop_mode() == "insert", "pop should restore previous mode")
            canopy.assert(canopy.pop_mode() == "", "pop should return to default mode")
        "#,
        )?;

        assert_eq!(harness.canopy.input_mode(), "");
        Ok(())
    }

    #[test]
    fn luau_observation_helpers_expose_runtime_state() -> Result<()> {
        let mut harness = Harness::builder(ApiRoot).size(20, 5).build()?;
        harness.render()?;

        harness.canopy.eval_script(
            r##"
            canopy.send_key("x")

            local cells = canopy.screen_cells()
            canopy.assert(#cells > 0, "screen cells should include rows")
            canopy.assert(cells[1][1].fg:sub(1, 1) == "#", "cell fg should be RGB text")

            local region = canopy.screen_region(0, 0, 8, 1)
            canopy.assert(type(region) == "string", "screen region should be text")

            local leaves = canopy.find_nodes("api_root/api_leaf")
            canopy.assert(canopy.node_region(leaves[1]):find("0") ~= nil, "node region should crop text")

            local trace = canopy.route_trace()
            canopy.assert(#trace > 0, "route trace should record the injected key")

            local dump = canopy.diagnostic_dump(leaves[1])
            canopy.assert(dump:find("node tree") ~= nil, "diagnostic dump should include the tree")

            local help = canopy.help_snapshot()
            canopy.assert(help.focus ~= nil, "help snapshot should include focus")
            canopy.assert(#help.commands > 0, "help snapshot should include commands")

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
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.register_default_bindings("api_leaf", r#"canopy.log("from bindings")"#)?;
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
        assert_eq!(nested.origin, "default-bindings:api_leaf");
        assert_eq!(nested.logs, vec!["from bindings".to_string()]);
        let outer = &journal[1];
        assert_eq!(outer.origin, "eval");
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
        let root = test_dir("startup");
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

        let mut canopy = raw_canopy_with_leaf()?;
        canopy.set_user_script_root(&user_root)?;
        canopy.set_project_script_root(&project_root)?;
        canopy.register_startup_script(
            "app",
            r#"
            function setup()
                api_leaf.set(1)
            end
        "#,
        )?;

        assert_eq!(canopy.run_startup_scripts()?, 3);
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(33)
        );
        assert_eq!(canopy.run_startup_scripts()?, 0);

        let _removed = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn startup_scripts_require_setup_global() -> Result<()> {
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.register_startup_script("app", "api_leaf.set(1)")?;

        let error = canopy
            .run_startup_scripts()
            .expect_err("startup script without setup should fail typechecking");
        assert!(
            error
                .to_string()
                .contains("startup/app:0:0: Required global 'setup'"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn startup_scripts_accept_additional_global_requirements() -> Result<()> {
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.require_startup_global("configure_workspace", "() -> ()")?;
        canopy.register_startup_script(
            "app",
            r#"
            function configure_workspace()
                api_leaf.set(1)
            end

            function setup()
                configure_workspace()
            end
        "#,
        )?;

        assert_eq!(canopy.run_startup_scripts()?, 1);
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(1)
        );
        Ok(())
    }

    #[test]
    fn startup_failure_releases_registered_callbacks() -> Result<()> {
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.register_startup_script(
            "failing",
            r#"
            function setup()
                canopy.bind("x", function() api_leaf.set(99) end)
                error("startup failed")
            end
        "#,
        )?;

        canopy
            .run_startup_scripts()
            .expect_err("startup execution should fail");
        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(0)
        );
        Ok(())
    }

    #[test]
    fn startup_retry_skips_successes_and_restores_prior_registrations() -> Result<()> {
        let mut canopy = raw_canopy_with_leaf()?;
        canopy.register_startup_script(
            "first",
            r#"
            function setup()
                api_leaf.set(1)
                canopy.bind("x", function() api_leaf.set(7) end)
            end
        "#,
        )?;
        canopy.register_startup_script(
            "second",
            r#"
            function setup()
                api_leaf.set(api_leaf.get() + 10)
                canopy.bind("x", function() api_leaf.set(99) end)
                canopy.on_start(function() api_leaf.set(88) end)
                error("second failed")
            end
        "#,
        )?;

        canopy
            .run_startup_scripts()
            .expect_err("second startup should fail");
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(11)
        );
        canopy
            .run_startup_scripts()
            .expect_err("second startup retry should fail");
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(21)
        );

        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(7)
        );

        let (_, mut render) = TestRender::create();
        canopy.set_root_size(Size::new(10, 1))?;
        canopy.render(&mut render)?;
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(7)
        );

        let first_runs = canopy
            .script_journal()
            .iter()
            .filter(|entry| entry.origin == "startup:first")
            .count();
        let second_runs = canopy
            .script_journal()
            .iter()
            .filter(|entry| entry.origin == "startup:second")
            .count();
        assert_eq!(first_runs, 1);
        assert_eq!(second_runs, 2);
        Ok(())
    }

    #[test]
    fn script_module_declarations_must_conform() -> Result<()> {
        let root = test_dir("conformance");
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

        let mut canopy = raw_canopy_with_leaf()?;
        canopy.set_project_script_root(&project_root)?;
        let err = canopy
            .finalize_api()
            .expect_err("mismatched declaration should fail finalization");
        assert!(err.to_string().contains("settings.d.luau"));
        assert_eq!(canopy.script_api_state(), ScriptApiState::Open);
        assert!(canopy.script_api().is_err());

        write_script(
            &project_root.join("settings.d.luau"),
            r#"
            declare module: {
                value: string,
            }
        "#,
        );
        canopy.finalize_api()?;
        assert_eq!(canopy.script_api_state(), ScriptApiState::Ready);
        assert!(canopy.script_api().is_ok());

        let _removed = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn run_config_loads_named_files_with_relative_requires() -> Result<()> {
        let root = test_dir("config");
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
            api_leaf.set(lib.value)
            canopy.bind("x", function() api_leaf.set(99) end)
        "#,
        );

        let mut canopy = raw_canopy_with_leaf()?;
        canopy.set_project_script_root(&project_root)?;
        canopy.run_config(&config)?;

        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(44)
        );

        write_script(&project_root.join("lib.luau"), "return { value = 45 }");
        assert!(canopy.invalidate_project_script_modules().is_some());
        canopy.eval_script(r#"canopy.send_key("x")"#)?;
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(44)
        );
        canopy.run_config(&config)?;
        assert_eq!(
            canopy.eval_script_value("return api_leaf.get()")?,
            ArgValue::Int(45)
        );

        let _removed = fs::remove_dir_all(root);
        Ok(())
    }
}
