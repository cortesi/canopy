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

    #[derive(Debug, Clone, Serialize, Deserialize, CommandArg)]
    struct Payload {
        count: usize,
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

        harness.script(r#"script_target.set_optional({ count = 14 })"#)?;
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

        let err = harness.script(r#"canopy.cmd("script_target::set", { foo = 11 })"#);
        assert!(matches!(err, Err(Error::Script(_))));

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
}
