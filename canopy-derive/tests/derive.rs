use std::marker::PhantomData;

use canopy::{
    self,
    commands::{ArgTypes, Args, CommandInvocation, CommandNode, ReturnSpec, ReturnTypes},
    tutils::*,
    Result, StatefulNode,
};
use canopy_derive::{command, derive_commands};

#[cfg(test)]
use pretty_assertions::assert_eq;

#[test]
fn statefulnode() {
    #[derive(canopy::StatefulNode)]
    struct FooBar {
        state: canopy::NodeState,
    }

    let f = FooBar {
        state: canopy::NodeState::default(),
    };

    assert_eq!(f.name(), "foo_bar");
}

#[test]
fn commands() {
    #[derive(canopy::StatefulNode)]
    struct Foo {
        state: canopy::NodeState,
        a_triggered: bool,
        b_triggered: bool,
        c_triggered: bool,
        naked_str_triggered: bool,
        core_isize: Option<isize>,
        naked_isize: Option<isize>,
    }

    impl canopy::Node for Foo {}

    struct Opaque {}

    #[derive_commands]
    impl Foo {
        #[command]
        /// This is a comment.
        /// Multiline too!
        fn a(&mut self, _core: &dyn canopy::Core) -> Result<()> {
            self.a_triggered = true;
            Ok(())
        }

        #[command]
        fn b(&mut self, _core: &dyn canopy::Core) -> canopy::Result<()> {
            self.b_triggered = true;
            Ok(())
        }

        #[command]
        fn c(&mut self, _core: &dyn canopy::Core) {
            self.c_triggered = true;
        }

        #[command(ignore_result)]
        fn d(&mut self, _core: &dyn canopy::Core) -> Opaque {
            self.c_triggered = true;
            Opaque {}
        }

        #[command(ignore_result)]
        fn f_core_isize(&mut self, _core: &dyn canopy::Core, i: isize) -> Opaque {
            self.core_isize = Some(i);
            Opaque {}
        }

        #[command]
        fn naked_isize(&mut self, i: isize) {
            self.naked_isize = Some(i);
        }

        #[command]
        fn naked_str(&mut self, _core: &dyn canopy::Core) -> String {
            self.naked_str_triggered = true;
            "".into()
        }

        #[command]
        fn result_str(&mut self, _core: &dyn canopy::Core) -> canopy::Result<String> {
            self.naked_str_triggered = true;
            Ok("".into())
        }

        #[command]
        fn nocore(&mut self) -> canopy::Result<String> {
            Ok("".into())
        }
    }

    assert_eq!(
        Foo::commands(),
        [
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "a".to_string(),
                docs: "This is a comment.\nMultiline too!".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, true),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "b".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, true),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "c".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, false),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "d".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, false),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "f_core_isize".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, false),
                args: vec![ArgTypes::Core, ArgTypes::ISize],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "naked_isize".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::Void, false),
                args: vec![ArgTypes::ISize],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "naked_str".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::String, false),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "result_str".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::String, true),
                args: vec![ArgTypes::Core],
            },
            canopy::commands::CommandSpec {
                node: "foo".try_into().unwrap(),
                command: "nocore".to_string(),
                docs: "".to_string(),
                ret: ReturnSpec::new(ReturnTypes::String, true),
                args: vec![],
            }
        ]
    );
    let mut f = Foo {
        state: canopy::NodeState::default(),
        a_triggered: false,
        b_triggered: false,
        c_triggered: false,
        naked_str_triggered: false,
        core_isize: None,
        naked_isize: None,
    };

    let mut dc = DummyCore {};

    f.dispatch(
        &mut dc,
        &CommandInvocation {
            node: "foo".try_into().unwrap(),
            command: "a".try_into().unwrap(),
            args: vec![],
        },
    )
    .unwrap();
    assert!(f.a_triggered);

    f.dispatch(
        &mut dc,
        &CommandInvocation {
            node: "foo".try_into().unwrap(),
            command: "c".try_into().unwrap(),
            args: vec![],
        },
    )
    .unwrap();
    assert!(f.c_triggered);

    f.dispatch(
        &mut dc,
        &CommandInvocation {
            node: "foo".try_into().unwrap(),
            command: "f_core_isize".try_into().unwrap(),
            args: vec![Args::Core, Args::ISize(3)],
        },
    )
    .unwrap();
    assert_eq!(f.core_isize, Some(3));

    f.dispatch(
        &mut dc,
        &CommandInvocation {
            node: "foo".try_into().unwrap(),
            command: "naked_isize".try_into().unwrap(),
            args: vec![Args::ISize(3)],
        },
    )
    .unwrap();
    assert_eq!(f.naked_isize, Some(3));

    #[derive(canopy::StatefulNode)]
    struct Bar<N>
    where
        N: canopy::Node,
    {
        state: canopy::NodeState,
        a_triggered: bool,
        p: PhantomData<N>,
    }

    #[derive_commands]
    impl<N> Bar<N>
    where
        N: canopy::Node,
    {
        #[command]
        fn a(&mut self, _core: &dyn canopy::Core) -> canopy::Result<()> {
            self.a_triggered = true;
            Ok(())
        }
    }

    assert_eq!(
        Bar::<Foo>::commands(),
        [canopy::commands::CommandSpec {
            node: "bar".try_into().unwrap(),
            command: "a".to_string(),
            docs: "".to_string(),
            ret: ReturnSpec::new(ReturnTypes::Void, true),
            args: vec![ArgTypes::Core],
        },]
    );
}
