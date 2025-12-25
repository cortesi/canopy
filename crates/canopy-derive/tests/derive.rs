//! Derive macro integration tests.

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use canopy::{
        self, ViewContext,
        commands::{
            ArgTypes, Args, CommandInvocation, CommandNode, CommandSpec, ReturnSpec, ReturnTypes,
        },
        error::{Error, Result},
        geom::Rect,
        render::Render,
        testing::dummyctx::DummyContext,
        widget::Widget,
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
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
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
        fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn commands() {
        assert_eq!(
            Foo::commands(),
            [
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "a".to_string(),
                    docs: "This is a comment.\nMultiline too!".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, true),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "b".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, true),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "c".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, false),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "d".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, false),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "f_core_isize".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, false),
                    args: vec![ArgTypes::Context, ArgTypes::ISize],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "naked_isize".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, false),
                    args: vec![ArgTypes::ISize],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "naked_str".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::String, false),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "result_str".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::String, true),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "ignored_result".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::Void, true),
                    args: vec![ArgTypes::Context],
                },
                CommandSpec {
                    node: "foo".try_into().unwrap(),
                    command: "nocore".to_string(),
                    docs: "".to_string(),
                    ret: ReturnSpec::new(ReturnTypes::String, true),
                    args: vec![],
                }
            ]
        );

        let mut f = Foo {
            a_triggered: false,
            b_triggered: false,
            c_triggered: false,
            naked_str_triggered: false,
            ignored_result_triggered: false,
            core_isize: None,
            naked_isize: None,
        };

        let mut dc = DummyContext::default();

        f.dispatch(
            &mut dc,
            &CommandInvocation {
                node: "foo".try_into().unwrap(),
                command: "a".into(),
                args: vec![Args::Context],
            },
        )
        .unwrap();

        assert!(f.a_triggered);

        let err = f.dispatch(
            &mut dc,
            &CommandInvocation {
                node: "foo".try_into().unwrap(),
                command: "ignored_result".into(),
                args: vec![Args::Context],
            },
        );
        assert!(matches!(err, Err(Error::Invalid(_))));
        assert!(f.ignored_result_triggered);

        let err = f.dispatch(
            &mut dc,
            &CommandInvocation {
                node: "foo".try_into().unwrap(),
                command: "naked_isize".into(),
                args: vec![],
            },
        );
        assert!(matches!(err, Err(Error::Invalid(_))));

        let mut bar = Bar::<Foo> {
            a_triggered: false,
            p: PhantomData,
        };
        bar.a(&dc).unwrap();
        assert!(bar.a_triggered);
    }
}
