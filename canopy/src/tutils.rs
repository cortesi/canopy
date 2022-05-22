#[cfg(test)]
pub mod utils {
    use std::cell::RefCell;

    use crate::{self as canopy, BackendControl};
    use crate::{
        derive_commands,
        event::{key, mouse},
        fit,
        geom::Expanse,
        widgets::list::ListItem,
        Node, NodeName, NodeState, Outcome, Render, Result, StatefulNode,
    };

    pub const K_ANY: key::Key = key::Key(None, key::KeyCode::Char('a'));

    #[derive(Debug, PartialEq, Clone)]
    pub struct State {
        pub path: Vec<String>,
    }

    impl State {
        pub fn new() -> Self {
            State { path: vec![] }
        }
        pub fn reset(&mut self) {
            self.path = vec![];
        }
        pub fn add_event(&mut self, n: &NodeName, evt: &str, result: Outcome) {
            let outcome = match result {
                Outcome::Handle { .. } => "handle",
                Outcome::Ignore { .. } => "ignore",
            };
            self.path.push(format!("{}@{}->{}", n, evt, outcome))
        }
    }

    thread_local! {
        pub (crate) static TSTATE: RefCell<State> = RefCell::new(State::new());
    }

    pub fn reset_state() {
        TSTATE.with(|s| {
            s.borrow_mut().reset();
        });
    }

    pub fn get_state() -> State {
        TSTATE.with(|s| -> State { s.borrow().clone() })
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TRoot {
        state: NodeState,

        pub next_outcome: Option<Outcome>,
        pub a: TBranch,
        pub b: TBranch,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TBranch {
        state: NodeState,

        pub next_outcome: Option<Outcome>,
        pub a: TLeaf,
        pub b: TLeaf,
    }

    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TLeaf {
        state: NodeState,

        pub next_outcome: Option<Outcome>,
    }

    #[derive_commands]
    impl Node for TLeaf {
        fn accept_focus(&mut self) -> bool {
            true
        }
        fn render(&mut self, r: &mut Render) -> Result<()> {
            r.text(
                "any",
                self.vp().view_rect().first_line(),
                &format!("<{}>", self.name().clone()),
            )
        }
        fn handle_key(&mut self, _: &mut dyn BackendControl, _: key::Key) -> Result<Outcome> {
            self.handle("key")
        }
        fn handle_mouse(&mut self, _: &mut dyn BackendControl, _: mouse::Mouse) -> Result<Outcome> {
            self.handle("mouse")
        }
    }

    impl Node for TBranch {
        fn accept_focus(&mut self) -> bool {
            true
        }

        fn render(&mut self, r: &mut Render) -> Result<()> {
            let parts = self.vp().split_vertical(2)?;
            fit(&mut self.a, parts[0])?;
            fit(&mut self.b, parts[1])?;

            r.text(
                "any",
                self.vp().view_rect().first_line(),
                &format!("<{}>", self.name().clone()),
            )
        }

        fn handle_key(&mut self, _: &mut dyn BackendControl, _: key::Key) -> Result<Outcome> {
            self.handle("key")
        }

        fn handle_mouse(&mut self, _: &mut dyn BackendControl, _: mouse::Mouse) -> Result<Outcome> {
            self.handle("mouse")
        }

        fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.a)?;
            f(&mut self.b)?;
            Ok(())
        }
    }

    impl Node for TRoot {
        fn accept_focus(&mut self) -> bool {
            true
        }

        fn render(&mut self, r: &mut Render) -> Result<()> {
            let parts = self.vp().split_horizontal(2)?;
            fit(&mut self.a, parts[0])?;
            fit(&mut self.b, parts[1])?;

            r.text(
                "any",
                self.vp().view_rect().first_line(),
                &format!("<{}>", self.name().clone()),
            )
        }

        fn handle_key(&mut self, _: &mut dyn BackendControl, _: key::Key) -> Result<Outcome> {
            self.handle("key")
        }

        fn handle_mouse(&mut self, _: &mut dyn BackendControl, _: mouse::Mouse) -> Result<Outcome> {
            self.handle("mouse")
        }

        fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
            f(&mut self.a)?;
            f(&mut self.b)?;
            Ok(())
        }
    }

    impl TLeaf {
        pub fn new(name: &str) -> Self {
            let mut n = TLeaf {
                state: NodeState::default(),
                next_outcome: None,
            };
            n.set_name(name.try_into().unwrap());
            n
        }

        pub fn make_mouse_event(&self) -> Result<mouse::Mouse> {
            let a = self.vp().screen_rect();
            Ok(mouse::Mouse {
                action: Some(mouse::MouseAction::Down),
                button: Some(mouse::Button::Left),
                modifiers: None,
                loc: a.tl,
            })
        }

        fn handle(&mut self, evt: &str) -> Result<Outcome> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::Ignore
            };
            TSTATE.with(|s| {
                s.borrow_mut().add_event(&self.name(), evt, ret.clone());
            });
            Ok(ret)
        }
    }

    #[derive_commands]
    impl TBranch {
        pub fn new(name: &str) -> Self {
            let mut n = TBranch {
                state: NodeState::default(),
                a: TLeaf::new(&(name.to_owned() + "_" + "la")),
                b: TLeaf::new(&(name.to_owned() + "_" + "lb")),
                next_outcome: None,
            };
            n.set_name(name.try_into().unwrap());
            n
        }
        fn handle(&mut self, evt: &str) -> Result<Outcome> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::Ignore
            };
            TSTATE.with(|s| {
                s.borrow_mut().add_event(&self.name(), evt, ret.clone());
            });
            Ok(ret)
        }
    }

    #[derive_commands]
    impl TRoot {
        pub fn new() -> Self {
            let mut n = TRoot {
                state: NodeState::default(),
                a: TBranch::new("ba"),
                b: TBranch::new("bb"),
                next_outcome: None,
            };
            n.set_name("r".try_into().unwrap());
            n
        }
        fn handle(&mut self, evt: &str) -> Result<Outcome> {
            let ret = if let Some(x) = self.next_outcome.clone() {
                self.next_outcome = None;
                x
            } else {
                Outcome::Ignore
            };
            TSTATE.with(|s| {
                s.borrow_mut().add_event(&self.name(), evt, ret.clone());
            });
            Ok(ret)
        }
    }

    // A fixed-size test node
    #[derive(Debug, PartialEq, StatefulNode)]
    pub struct TFixed {
        state: NodeState,
        pub w: u16,
        pub h: u16,
    }

    impl Node for TFixed {
        fn fit(&mut self, _target: Expanse) -> Result<Expanse> {
            Ok(Expanse {
                w: self.w,
                h: self.h,
            })
        }
    }

    #[derive_commands]
    impl TFixed {
        pub fn new(w: u16, h: u16) -> Self {
            TFixed {
                state: NodeState::default(),
                w,
                h,
            }
        }
    }

    impl ListItem for TFixed {}
}
