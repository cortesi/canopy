use std::marker::PhantomData;

use canopy;
use canopy::actions::Actions;
use canopy_derive::action;
use canopy_derive::derive_actions;

#[test]
fn statefulnode() {
    #[derive(canopy::StatefulNode)]
    struct Foo {
        state: canopy::NodeState,
    }
}

#[test]
fn actions() {
    #[derive(canopy::StatefulNode)]
    struct Foo {
        state: canopy::NodeState,
        a_triggered: bool,
        b_triggered: bool,
    }

    impl canopy::Node for Foo {}

    #[derive_actions]
    impl Foo {
        #[action]
        /// This is a comment.
        /// Multiline too!
        fn a(&mut self) -> canopy::Result<()> {
            self.a_triggered = true;
            Ok(())
        }
        #[action]
        fn b(&mut self) -> canopy::Result<()> {
            self.b_triggered = true;
            Ok(())
        }
    }

    assert_eq!(
        Foo::actions(),
        [
            canopy::actions::Action {
                name: "a".to_string(),
                docs: " This is a comment.\n Multiline too!".to_string()
            },
            canopy::actions::Action {
                name: "b".to_string(),
                docs: "".to_string(),
            }
        ]
    );
    let mut f = Foo {
        state: canopy::NodeState::default(),
        a_triggered: false,
        b_triggered: false,
    };
    f.dispatch("a").unwrap();
    assert!(f.a_triggered);

    #[derive(canopy::StatefulNode)]
    struct Bar<N>
    where
        N: canopy::Node,
    {
        state: canopy::NodeState,
        a_triggered: bool,
        p: PhantomData<N>,
    }

    #[derive_actions]
    impl<N> Bar<N>
    where
        N: canopy::Node,
    {
        #[action]
        fn a(&mut self) -> canopy::Result<()> {
            self.a_triggered = true;
            Ok(())
        }
    }

    assert_eq!(
        Bar::<Foo>::actions(),
        [canopy::actions::Action {
            name: "a".to_string(),
            docs: "".to_string()
        },]
    );
}
