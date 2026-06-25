//! Integration tests for script command helpers.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, CommandArg, Context, Loader, ReadContext, Widget, command,
        commands::ArgValue,
        derive_commands,
        error::{Error, Result},
        render::Render,
        testing::harness::Harness,
    };
    use serde::{Deserialize, Serialize};

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
    }

    #[derive_commands]
    impl ScriptTarget {
        fn new() -> Self {
            Self {
                value: 0,
                payload_value: 0,
                last_payload: None,
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
    }

    impl Widget for ScriptTarget {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
            Ok(())
        }
    }

    impl Loader for ScriptTarget {
        fn load(c: &mut Canopy) -> Result<()> {
            c.add_commands::<Self>()?;
            Ok(())
        }
    }

    #[test]
    fn script_helpers_dispatch_commands() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new()).size(10, 1).build()?;

        harness.script(r#"canopy.cmd("script_target::set", { count = 7 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 7);
        });

        harness.script(r#"script_target.set(12)"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 12);
        });

        harness.script(r#"canopy.cmd("script_target::set", { count = 13 })"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 13);
        });

        harness.script(r#"canopy.cmd("script_target::set", 9)"#)?;
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

        harness.script(r#"canopy.cmd("script_target::set_optional", { count = 14 })"#)?;
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
            .script(r#"canopy.cmd("script_target::set", { foo = 11 })"#)
            .expect_err("unknown named args are structured script errors");
        let Error::ScriptStructured { kind, command, .. } = err else {
            panic!("expected structured script error, got {err:?}");
        };
        assert_eq!(kind, "type_mismatch");
        assert_eq!(command, None);

        Ok(())
    }

    #[test]
    fn command_discovery_reports_contract_and_availability() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new()).size(10, 1).build()?;

        let command = harness.canopy.eval_script_value(
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
            .eval_script_value(r#"return canopy.resolve("script_target") ~= nil"#)?;
        assert_eq!(resolved, ArgValue::Bool(true));

        let forged_node = harness.canopy.eval_script_value(
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

        let error = harness.canopy.eval_script_value(
            r#"
            local ok, err = pcall(function()
                canopy.cmd("missing::command")
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

        let payload_param = harness.canopy.eval_script_value(
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
        let mut harness = Harness::builder(ScriptTarget::new()).size(10, 1).build()?;

        let value = harness
            .canopy
            .eval_script_value(r#"canopy.log("hello"); canopy.assert(true, "ok"); return 7"#)?;
        assert_eq!(value, ArgValue::Int(7));
        assert_eq!(harness.canopy.take_script_logs(), vec!["hello"]);

        let assertions = harness.canopy.take_script_assertions();
        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed);
        assert_eq!(assertions[0].message, "ok");

        Ok(())
    }

    #[test]
    fn on_start_hooks_run_after_first_render() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new()).size(10, 1).build()?;

        harness
            .canopy
            .eval_script("canopy.on_start(function() script_target.set(21) end)")?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 0);
        });

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
    fn recursive_command_arg_declarations_terminate() {
        use canopy::commands::{CommandType, DeclRegistry, decl};

        let mut builder = decl::Builder::new();
        let mut registry = DeclRegistry::new(&mut builder);
        TreePayload::luau_decls(&mut registry);
        TreePayload::luau_decls(&mut registry);

        let rendered = builder.finish().expect("valid declarations").render();
        assert_eq!(rendered.matches("export type TreePayload").count(), 1);
        assert!(rendered.contains("children: {TreePayload}"));
    }
}
