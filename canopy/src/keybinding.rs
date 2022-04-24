use crate::{Action, Actions};

/// The Keybindings struct manages the global set of key bindings for the app.
pub struct Keybindings {}

impl Keybindings {
    pub fn new() -> Self {
        Keybindings {}
    }

    fn load(&mut self, f: fn() -> Vec<Action>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as canopy;
    use crate::{action, derive_actions, Result};

    #[test]
    fn kb_load() -> Result<()> {
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

        let mut kb = Keybindings::new();
        kb.load(Foo::actions);

        Ok(())
    }
}
