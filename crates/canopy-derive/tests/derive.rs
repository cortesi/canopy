//! Derive macro integration tests.

#[cfg(test)]
mod tests {
    use std::{any::Any, marker::PhantomData};

    use canopy::{
        self, Widget,
        commands::{
            ArgValue, CommandArgs, CommandDispatchKind, CommandError, CommandNode,
            CommandParamKind, CommandReturnSpec,
        },
        error::{Error, Result},
        testing::dummyctx::DummyContext,
    };
    use canopy_derive::{command, derive_commands};
    use pretty_assertions::assert_eq;

    struct Opaque {}

    #[derive(Default)]
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
        /// Return a string.
        /// @return The generated string.
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

    impl Widget for Foo {}

    struct Collision;

    #[derive_commands]
    impl Collision {
        #[command]
        fn bindings(
            &mut self,
            target: String,
            ctx: &mut dyn canopy::Context,
            values: String,
            inv: Option<canopy::event::Event>,
            normalized: String,
            __canopy_param_0: String,
        ) -> String {
            assert!(inv.is_none());
            let _root = ctx.root_id();
            format!("{target}/{values}/{normalized}/{__canopy_param_0}")
        }

        #[command(ignore_result)]
        fn opaque(&mut self, fail: bool) -> std::result::Result<Opaque, Error> {
            if fail {
                Err(Error::Invalid("opaque failure".into()))
            } else {
                Ok(Opaque {})
            }
        }

        #[command]
        fn explicit(&mut self) -> std::result::Result<String, Error> {
            Ok("value".into())
        }
    }

    #[test]
    fn generated_bindings_preserve_parameter_names_and_order() {
        let mut target = Collision;
        let mut ctx = DummyContext::default();
        let spec = Collision::cmd_bindings();
        let names = ["target", "values", "normalized", "__canopy_param_0"];
        let values = ["a", "b", "c", "d"].map(|s| ArgValue::String(s.into()));
        let mut inv = spec.call_with(()).invocation();
        for args in [
            CommandArgs::Positional(values.to_vec()),
            CommandArgs::Named(names.into_iter().map(String::from).zip(values).collect()),
        ] {
            inv.args = args;
            assert_eq!(
                (spec.invoke)(Some(&mut target), &mut ctx, &inv).unwrap(),
                ArgValue::String("a/b/c/d".into()),
            );
        }
        assert_eq!(
            spec.params.iter().map(|p| p.name).collect::<Vec<_>>(),
            ["target", "values", "inv", "normalized", "__canopy_param_0"]
        );
    }

    #[test]
    fn explicit_result_dispatch_preserves_errors_and_values() {
        let mut target = Collision;
        let mut ctx = DummyContext::default();
        let spec = Collision::cmd_opaque();
        for fail in [false, true] {
            let mut inv = spec.call_with(()).invocation();
            inv.args = CommandArgs::Positional(vec![ArgValue::Bool(fail)]);
            let result = (spec.invoke)(Some(&mut target), &mut ctx, &inv);
            if fail {
                assert!(matches!(result, Err(CommandError::Exec(_))));
            } else {
                assert_eq!(result.unwrap(), ArgValue::Null);
            }
        }
        let spec = Collision::cmd_explicit();
        let inv = spec.call_with(()).invocation();
        assert_eq!(
            (spec.invoke)(Some(&mut target), &mut ctx, &inv).unwrap(),
            ArgValue::String("value".into())
        );
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

    impl<N> Widget for Bar<N> where N: Widget {}

    fn assert_cmd_a_metadata() {
        let cmd_a = Foo::cmd_a();
        assert_eq!(cmd_a.id.0, "foo::a");
        assert_eq!(cmd_a.name, "a");
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
            CommandReturnSpec::Value(spec)
                if spec.rust == "String" && spec.doc == Some("The generated string.")
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
        let mut f = Foo::default();
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_a().call_with(()).invocation();
        let out = (Foo::cmd_a().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv).unwrap();

        assert_eq!(out, ArgValue::Null);
        assert!(f.a_triggered);
    }

    #[test]
    fn missing_args_error() {
        let mut f = Foo::default();
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_naked_isize().call_with(()).invocation();
        let err = (Foo::cmd_naked_isize().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv)
            .unwrap_err();

        assert!(matches!(err, CommandError::ArityMismatch { .. }));
        assert!(f.naked_isize.is_none());
    }

    #[test]
    fn ignored_result_wraps_errors() {
        let mut f = Foo::default();
        let mut ctx = DummyContext::default();
        let inv = Foo::cmd_ignored_result().call_with(()).invocation();
        let err = (Foo::cmd_ignored_result().invoke)(Some(&mut f as &mut dyn Any), &mut ctx, &inv)
            .unwrap_err();

        assert!(matches!(err, CommandError::Exec(_)));
        assert!(f.ignored_result_triggered);
    }
}
