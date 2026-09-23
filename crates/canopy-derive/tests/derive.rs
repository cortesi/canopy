//! Derive macro integration tests.

#[cfg(test)]
mod tests {
    use std::{any::Any, marker::PhantomData, result};

    use canopy::{
        self, Canopy, CanopyBuilder, Context, ViewContext, Widget,
        commands::{
            ArgValue, CommandArgs, CommandError, CommandNode, CommandParamKind, CommandRequirement,
            CommandReturnSpec, CommandStatus,
        },
        error::{Error, Result},
        event::{Event, mouse::MouseEvent},
    };
    use canopy_derive::derive_commands;
    use pretty_assertions::assert_eq;
    use serde::de::DeserializeOwned;

    /// Build an application with no registrations.
    fn app() -> Canopy {
        CanopyBuilder::new()
            .build()
            .expect("an empty application builds")
    }

    /// Run `f` against the root context of an empty application.
    fn with_ctx<R>(f: impl FnOnce(&mut dyn Context) -> R) -> R {
        app()
            .with_root_context(|ctx| Ok(f(ctx)))
            .expect("root context is available")
    }

    /// Run `f` against the root view of an empty application.
    fn with_view<R>(f: impl FnOnce(&dyn ViewContext) -> R) -> R {
        app().with_root_view(f)
    }

    #[derive(serde::Serialize, serde::Deserialize, canopy_derive::CommandArg)]
    struct TreeArgument {
        label: String,
        children: Vec<Self>,
    }

    #[derive(serde::Serialize, serde::Deserialize, canopy_derive::CommandArg)]
    #[serde(bound = "T: serde::Serialize + serde::de::DeserializeOwned")]
    struct GenericArgument<T>
    where
        T: serde::Serialize + DeserializeOwned + 'static,
    {
        value: T,
    }

    #[test]
    fn command_arg_encodes_structural_recursive_and_generic_values() {
        use std::collections::BTreeMap;

        use canopy::commands::ToArgValue;

        let tree = TreeArgument {
            label: "root".into(),
            children: vec![TreeArgument {
                label: "child".into(),
                children: vec![],
            }],
        }
        .to_arg_value();
        let ArgValue::Map(fields) = tree else {
            panic!("expected record");
        };
        assert_eq!(fields["label"], ArgValue::String("root".into()));
        let ArgValue::Array(children) = &fields["children"] else {
            panic!("expected children");
        };
        assert_eq!(children.len(), 1);
        let ArgValue::Map(child) = &children[0] else {
            panic!("expected child record");
        };
        assert_eq!(child["label"], ArgValue::String("child".into()));
        assert_eq!(
            GenericArgument { value: 7i64 }.to_arg_value(),
            ArgValue::Map(BTreeMap::from([("value".into(), ArgValue::Int(7))])),
        );
    }

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
            &self,
            target: String,
            ctx: &mut dyn canopy::Context,
            values: String,
            args: Option<MouseEvent>,
            normalized: String,
            __canopy_param_0: String,
        ) -> String {
            assert!(args.is_none());
            ctx.set_focus(ctx.node_id()).expect("the command node accepts focus");
            [target, values, normalized, __canopy_param_0].join("/")
        }

        #[command(ignore_result)]
        fn opaque(&self, fail: bool) -> result::Result<Opaque, Error> {
            if fail {
                Err(Error::Invalid("opaque failure".into()))
            } else {
                Ok(Opaque {})
            }
        }

        #[command]
        fn explicit(&self) -> result::Result<String, Error> {
            Ok("value".into())
        }
    }

    #[test]
    fn generated_bindings_preserve_parameter_names_and_order() {
        let mut target = Collision;
        let spec = Collision::spec_bindings();
        let names = ["target", "values", "normalized", "__canopy_param_0"];
        let values = ["a", "b", "c", "d"].map(|s| ArgValue::String(s.into()));
        for args in [
            CommandArgs::Positional(values.to_vec()),
            CommandArgs::Named(names.into_iter().map(String::from).zip(values).collect()),
        ] {
            assert_eq!(
                with_ctx(|ctx| (spec.invoke)(&mut target, ctx, &args)).unwrap(),
                ArgValue::String("a/b/c/d".into()),
            );
        }
        assert_eq!(
            spec.params.iter().map(|p| p.name).collect::<Vec<_>>(),
            ["target", "values", "args", "normalized", "__canopy_param_0"]
        );
    }

    #[test]
    fn explicit_result_dispatch_preserves_errors_and_values() {
        let mut target = Collision;
        let spec = Collision::spec_opaque();
        for fail in [false, true] {
            let args = CommandArgs::Positional(vec![ArgValue::Bool(fail)]);
            let result = with_ctx(|ctx| (spec.invoke)(&mut target, ctx, &args));
            if fail {
                assert!(matches!(result, Err(CommandError::Exec(_))));
            } else {
                assert_eq!(result.unwrap(), ArgValue::Null);
            }
        }
        let spec = Collision::spec_explicit();
        assert_eq!(
            with_ctx(|ctx| (spec.invoke)(&mut target, ctx, &CommandArgs::default())).unwrap(),
            ArgValue::String("value".into())
        );
    }

    struct Eligible {
        enabled: bool,
    }

    #[derive_commands]
    impl Eligible {
        fn can_update(&self, _ctx: &dyn canopy::ViewContext) -> Result<CommandStatus> {
            Ok(if self.enabled {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("no selection".into())
            })
        }

        #[command(enabled = "can_update")]
        fn update(
            &self,
            _ctx: &mut dyn canopy::Context,
            value: Option<String>,
            _event: Event,
            _mouse: MouseEvent,
            _optional_mouse: Option<MouseEvent>,
        ) -> Option<String> {
            value
        }
    }

    #[test]
    fn typed_builders_preserve_arguments_and_omit_injections() {
        let call = Eligible::call_update(Some("selected".into()));
        assert_eq!(call.id, Eligible::spec_update().id);
        assert_eq!(call.target, None);
        assert_eq!(
            call.args,
            CommandArgs::Positional(vec![ArgValue::String("selected".into())])
        );
        assert_eq!(
            Eligible::call_update(None).args,
            CommandArgs::Positional(vec![ArgValue::Null])
        );
        assert_eq!(Bar::<Foo>::call_a().id, Bar::<Foo>::spec_a().id);
        let call = Collision::call_bindings("a".into(), "b".into(), "c".into(), "d".into());
        let mut target = Collision;
        assert_eq!(
            with_ctx(|ctx| (Collision::spec_bindings().invoke)(&mut target, ctx, &call.args))
                .unwrap(),
            ArgValue::String("a/b/c/d".into())
        );
    }

    #[test]
    fn status_shim_checks_target_and_reads_current_eligibility() -> Result<()> {
        let status = Eligible::spec_update().status.expect("eligibility hook");
        let mut target = Eligible { enabled: false };
        assert_eq!(
            with_view(|ctx| status(&target, ctx))?,
            CommandStatus::Disabled("no selection".into())
        );
        target.enabled = true;
        assert_eq!(
            with_view(|ctx| status(&target, ctx))?,
            CommandStatus::Enabled
        );
        assert!(with_view(|ctx| status(&Collision, ctx)).is_err());
        assert!(Foo::spec_a().status.is_none());
        Ok(())
    }

    #[test]
    fn injection_metadata_uses_requirements_and_preserves_optional_parameters() {
        let requirements = Eligible::spec_update()
            .params
            .iter()
            .map(|param| param.requirement.and_then(|requirement| requirement()))
            .collect::<Vec<_>>();
        assert_eq!(
            requirements,
            vec![
                None,
                Some(CommandRequirement::Event),
                Some(CommandRequirement::Mouse),
                Some(CommandRequirement::Mouse),
            ]
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

    fn assert_spec_a_metadata() {
        let spec_a = Foo::spec_a();
        assert_eq!(spec_a.id.0, "foo::a");
        assert_eq!(spec_a.name, "a");
        assert_eq!(spec_a.owner, "foo");
        assert!(spec_a.params.is_empty());
        assert!(matches!(spec_a.ret, CommandReturnSpec::Unit));
    }

    fn assert_spec_isize_metadata() {
        let spec_isize = Foo::spec_f_core_isize();
        assert_eq!(spec_isize.params.len(), 1);
        let param = spec_isize.params[0];
        assert_eq!(param.name, "i");
        assert_eq!(param.kind, CommandParamKind::User);
        assert_eq!(param.ty.rust, "isize");

        let spec_naked = Foo::spec_naked_isize();
        assert_eq!(spec_naked.params.len(), 1);
    }

    fn assert_spec_return_metadata() {
        let spec_str = Foo::spec_naked_str();
        assert!(matches!(
            spec_str.ret,
            CommandReturnSpec::Value(spec)
                if spec.rust == "String" && spec.doc == Some("The generated string.")
        ));

        let spec_result = Foo::spec_result_str();
        assert!(matches!(
            spec_result.ret,
            CommandReturnSpec::Value(spec) if spec.rust == "String"
        ));

        let spec_nocore = Foo::spec_nocore();
        assert!(spec_nocore.params.is_empty());
        assert!(matches!(
            spec_nocore.ret,
            CommandReturnSpec::Value(spec) if spec.rust == "String"
        ));
    }

    fn assert_spec_ignore_result() {
        let spec_ignore = Foo::spec_ignored_result();
        assert!(matches!(spec_ignore.ret, CommandReturnSpec::Unit));
    }

    fn assert_spec_bar_metadata() {
        let spec_bar = Bar::<Foo>::spec_a();
        assert_eq!(spec_bar.id.0, "bar::a");
    }

    #[test]
    fn commands() {
        let commands = Foo::commands();
        assert_eq!(commands.len(), 10);

        assert_spec_a_metadata();
        assert_spec_isize_metadata();
        assert_spec_return_metadata();
        assert_spec_ignore_result();
        assert_spec_bar_metadata();
    }

    struct ConditionalApp;

    #[derive_commands]
    impl ConditionalApp {
        #[cfg(any())]
        #[command(enabled = "absent_eligibility")]
        fn absent(&self, _value: MissingType) {}

        #[cfg_attr(all(), cfg_attr(all(), cfg(any())), inline)]
        #[command]
        fn nested_absent(&self, _value: MissingType) {}

        #[cfg(not(any()))]
        #[cfg_attr(all(), inline)]
        #[command]
        fn present(&self, _value: i64) {}
    }

    #[derive_commands]
    #[cfg(any())]
    impl MissingType {
        #[command]
        fn absent(&self) {}
    }

    #[test]
    fn conditional_commands_drop_absent_methods() {
        assert_eq!(ConditionalApp::commands().len(), 1);
        let _ = ConditionalApp::call_present(3);
        let _ = ConditionalApp::spec_present();
    }

    #[test]
    fn invoke_dispatches() {
        let mut f = Foo::default();
        let call = Foo::call_a();
        let out = with_ctx(|ctx| (Foo::spec_a().invoke)(&mut f as &mut dyn Any, ctx, &call.args))
            .unwrap();

        assert_eq!(out, ArgValue::Null);
        assert!(f.a_triggered);
    }

    #[test]
    fn missing_args_error() {
        let mut f = Foo::default();
        let args = CommandArgs::default();
        let err =
            with_ctx(|ctx| (Foo::spec_naked_isize().invoke)(&mut f as &mut dyn Any, ctx, &args))
                .unwrap_err();

        assert!(matches!(err, CommandError::ArityMismatch { .. }));
        assert!(f.naked_isize.is_none());
    }

    #[test]
    fn check_applies_arity_and_type_rules_without_a_target() {
        let check = Foo::spec_naked_isize().check;
        assert!(matches!(
            check(&CommandArgs::Positional(vec![])),
            Err(CommandError::ArityMismatch { .. })
        ));
        assert!(matches!(
            check(&CommandArgs::Positional(vec![ArgValue::String("x".into())])),
            Err(CommandError::TypeMismatch { .. })
        ));
        assert!(matches!(
            check(&CommandArgs::Positional(vec![
                ArgValue::Int(1),
                ArgValue::Int(2)
            ])),
            Err(CommandError::ArityMismatch { .. })
        ));
        assert!(check(&CommandArgs::Positional(vec![ArgValue::Int(3)])).is_ok());
    }

    #[test]
    fn ignored_result_wraps_errors() {
        let mut f = Foo::default();
        let args = CommandArgs::default();
        let err =
            with_ctx(|ctx| (Foo::spec_ignored_result().invoke)(&mut f as &mut dyn Any, ctx, &args))
                .unwrap_err();

        assert!(matches!(err, CommandError::Exec(_)));
        assert!(f.ignored_result_triggered);
    }
}
