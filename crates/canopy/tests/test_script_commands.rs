//! Integration tests for script command helpers.

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, CommandArg, Context, Loader, ViewContext, command, derive_commands,
        error::{Error, Result},
        render::Render,
        testing::harness::Harness,
        widget::Widget,
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
        fn set_payload(&mut self, _ctx: &mut dyn Context, payload: Payload) {
            let Payload { count } = payload;
            self.payload_value = count;
            self.last_payload = Some(payload);
        }
    }

    impl Widget for ScriptTarget {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    impl Loader for ScriptTarget {
        fn load(c: &mut Canopy) {
            c.add_commands::<Self>();
        }
    }

    #[test]
    fn script_helpers_dispatch_commands() -> Result<()> {
        let mut harness = Harness::builder(ScriptTarget::new()).size(10, 1).build()?;

        harness.script(r#"cmd_named("script_target::set", #{count: 7})"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 7);
        });

        harness.script(r#"script_target::set(12)"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 12);
        });

        harness.script(r#"script_target::set(#{count: 13})"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 13);
        });

        harness.script(r#"cmdv("script_target::set", [9])"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 9);
        });

        harness.script(r#"script_target::set(#{count: 5})"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.value, 5);
        });

        harness.script(r#"script_target::set_payload(#{count: 3})"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.payload_value, 3);
        });

        harness.script(r#"script_target::set_payload(#{count: 4})"#)?;
        harness.with_root_widget::<ScriptTarget, _>(|target| {
            assert_eq!(target.payload_value, 4);
        });

        let err = harness.script(r#"cmd_pos("script_target::set", #{count: 11})"#);
        assert!(matches!(err, Err(Error::Script(_))));

        Ok(())
    }
}
