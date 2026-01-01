//! Derive macro integration tests.

#[cfg(test)]
mod tests {
    use std::{any::Any, marker::PhantomData};

    use canopy::{
        self, ViewContext, Widget,
        commands::{
            ArgValue, CommandDispatchKind, CommandError, CommandNode, CommandParamKind,
            CommandReturnSpec,
        },
        error::{Error, Result},
        render::Render,
        testing::dummyctx::DummyContext,
    };
    use canopy_derive::{command, derive_commands};
    #[cfg(test)]
    use pretty_assertions::assert_eq;

    struct Opaque {}

    struct Foo {
        a_triggered: bool,
        b_triggered: bool,
        c_triggered: bool,
        naked_str_triggered: bool,
        ignored_result_triggered: bool,
        core_isize: Option<isize>,
        naked_isize: Option<isize>,
    }

    #[derive_commands]
    impl Foo {
        #[command]
        /// This is a comment.
        /// Multiline too!
        fn a(&mut self, _core: &mut dyn canopy::Context) -> Result<()> {
            self.a_triggered = true;
            Ok(())
        }

        #[command]
        fn b(&mut self, _core: &mut dyn canopy::Context) -> Result<()> {
            self.b_triggered = true;
            Ok(())
        }

        #[command]
        fn c(&mut self, _core: &mut dyn canopy::Context) {
            self.c_triggered = true;
        }

        #[command(ignore_result)]
        fn d(&mut self, _core: &mut dyn canopy::Context) -> Opaque {
            self.c_triggered = true;
            Opaque {}
        }

        #[command(ignore_result)]
        fn f_core_isize(&mut self, _core: &mut dyn canopy::Context, i: isize) -> Opaque {
            self.core_isize = Some(i);
            Opaque {}
        }

        #[command]
        fn naked_isize(&mut self, i: isize) {
            self.naked_isize = Some(i);
        }

        #[command]
        fn naked_str(&mut self, _core: &mut dyn canopy::Context) -> String {
            self.naked_str_triggered = true;
            "".into()
        }

        #[command]
        fn result_str(&mut self, _core: &mut dyn canopy::Context) -> Result<String> {
            self.naked_str_triggered = true;
            Ok("".into())
        }

        #[command(ignore_result)]
        fn ignored_result(&mut self, _core: &mut dyn canopy::Context) -> Result<String> {
            self.ignored_result_triggered = true;
            Err(Error::Invalid("boom".into()))
        }

        #[command]
        fn nocore(&self) -> Result<String> {
            Ok("".into())
        }
    }

    impl Widget for Foo {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    struct Bar<N>
    where
        N: Widget,
    {
        a_triggered: bool,
        p: PhantomData<N>,
    }

    #[derive_commands]
    impl<N> Bar<N>
    where
        N: Widget,
    {
        #[command]
        fn a(&mut self, _core: &dyn canopy::Context) -> Result<()> {
            self.a_triggered = true;
            Ok(())
        }
    }

    impl<N> Widget for Bar<N>
    where
        N: Widget,
    {
        fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    fn assert_cmd_a_metadata() {
        let cmd_a = Foo::cmd_a();
        assert_eq!(cmd_a.id.0, "foo::a");
        assert!(matches!(
            cmd_a.dispatch,
            CommandDispatchKind::Node { owner } if owner == "foo"
        ));
        assert!(cmd_a.params.is_empty());
        assert!(matches!(cmd_a.ret, CommandReturnSpec::Unit));
    }

    fn assert_cmd_isize_metadata() {
        let cmd_isize = Foo::cmd_f_core_isize();
        assert_eq!(cmd_isize.params.len(), 1);
        let param = cmd_isize.params[0];
        assert_eq!(param.name, "i");
        assert_eq!(param.kind, CommandParamKind::User);
        assert_eq!(param.ty.rust, "isize");

        let cmd_naked = Foo::cmd_naked_isize();
        assert_eq!(cmd_naked.params.len(), 1);
    }

    fn assert_cmd_return_metadata() {
        let cmd_str = Foo::cmd_naked_str();
        assert!(matches!(
            cmd_str.ret,
            CommandReturnSpec::Value(spec) if spec.rust == "String"
        ));

        let cmd_result = Foo::cmd_result_str();
        assert!(matches!(
            cmd_result.ret,
            CommandReturnSpec::Value(spec) if spec.rust == "String"
        ));

        let cmd_nocore = Foo::cmd_nocore();
        assert!(cmd_nocore.params.is_empty());
        assert!(matches!(
            cmd_nocore.ret,
            CommandReturnSpec::Value(spec) if spec.rust == "String"
        ));
    }

    fn assert_cmd_ignore_result() {
        let cmd_ignore = Foo::cmd_ignored_result();
        assert!(matches!(cmd_ignore.ret, CommandReturnSpec::Unit));
    }

    fn assert_cmd_bar_metadata() {
        let cmd_bar = Bar::<Foo>::cmd_a();
        assert_eq!(cmd_bar.id.0, "bar::a");
    }

    #[test]
    fn commands() {
        let commands = Foo::commands();
        assert_eq!(commands.len(), 10);

        assert_cmd_a_metadata();
        assert_cmd_isize_metadata();
        assert_cmd_return_metadata();
        assert_cmd_ignore_result();
        assert_cmd_bar_metadata();
    }

    #[test]
    fn invoke_dispatches() {
        let mut f = Foo {
            a_triggered: false,
            b_triggered: false,
            c_triggered: false,
            naked_str_triggered: false,
            ignored_result_triggered: false,
            core_isize: None,
            naked_isize: None,
        };
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_a().call_with(()).invocation();
        let out = (Foo::cmd_a().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv).unwrap();

        assert_eq!(out, ArgValue::Null);
        assert!(f.a_triggered);
    }

    #[test]
    fn missing_args_error() {
        let mut f = Foo {
            a_triggered: false,
            b_triggered: false,
            c_triggered: false,
            naked_str_triggered: false,
            ignored_result_triggered: false,
            core_isize: None,
            naked_isize: None,
        };
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_naked_isize().call_with(()).invocation();
        let err = (Foo::cmd_naked_isize().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv)
            .unwrap_err();

        assert!(matches!(err, CommandError::ArityMismatch { .. }));
        assert!(f.naked_isize.is_none());
    }

    #[test]
    fn ignored_result_wraps_errors() {
        let mut f = Foo {
            a_triggered: false,
            b_triggered: false,
            c_triggered: false,
            naked_str_triggered: false,
            ignored_result_triggered: false,
            core_isize: None,
            naked_isize: None,
        };
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_ignored_result().call_with(()).invocation();
        let err = (Foo::cmd_ignored_result().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv)
            .unwrap_err();

        assert!(matches!(err, CommandError::Exec(_)));
        assert!(f.ignored_result_triggered);
    }
}
