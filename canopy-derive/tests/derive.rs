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
    println!("{:?}", Foo::actions());
    println!("POST");
}
