use std::marker::PhantomData;

use canopy;
use canopy::commands::{Command, CommandNode, ReturnTypes};
use canopy::StatefulNode;
use canopy_derive::command;
use canopy_derive::derive_commands;

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
    }

    impl canopy::Node for Foo {}

    #[derive_commands]
    impl Foo {
        #[command]
        /// This is a comment.
        /// Multiline too!
        fn a(&mut self) -> canopy::Result<()> {
            self.a_triggered = true;
            Ok(())
        }

        #[command]
        fn b(&mut self) -> canopy::Result<()> {
            self.b_triggered = true;
            Ok(())
        }

        #[command]
        fn c(&mut self) {
            self.c_triggered = true;
        }
    }

    assert_eq!(
        Foo::load_commands(None),
        [
            canopy::commands::CommandDefinition {
                node: "foo".try_into().unwrap(),
                command: "a".to_string(),
                docs: " This is a comment.\n Multiline too!".to_string(),
                return_type: ReturnTypes::Result,
            },
            canopy::commands::CommandDefinition {
                node: "foo".try_into().unwrap(),
                command: "b".to_string(),
                docs: "".to_string(),
                return_type: ReturnTypes::Result,
            },
            canopy::commands::CommandDefinition {
                node: "foo".try_into().unwrap(),
                command: "c".to_string(),
                docs: "".to_string(),
                return_type: ReturnTypes::Void,
            }
        ]
    );
    let mut f = Foo {
        state: canopy::NodeState::default(),
        a_triggered: false,
        b_triggered: false,
        c_triggered: false,
    };
    f.dispatch(&Command {
        node: "foo".try_into().unwrap(),
        command: "a".try_into().unwrap(),
    })
    .unwrap();
    assert!(f.a_triggered);

    f.dispatch(&Command {
        node: "foo".try_into().unwrap(),
        command: "c".try_into().unwrap(),
    })
    .unwrap();
    assert!(f.c_triggered);

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
        fn a(&mut self) -> canopy::Result<()> {
            self.a_triggered = true;
            Ok(())
        }
    }

    assert_eq!(
        Bar::<Foo>::load_commands(None),
        [canopy::commands::CommandDefinition {
            node: "bar".try_into().unwrap(),
            command: "a".to_string(),
            docs: "".to_string(),
            return_type: ReturnTypes::Result,
        },]
    );
    assert_eq!(
        Bar::<Foo>::load_commands(Some("xxx")),
        [canopy::commands::CommandDefinition {
            node: "xxx".try_into().unwrap(),
            command: "a".to_string(),
            docs: "".to_string(),
            return_type: ReturnTypes::Result,
        },]
    );
}
