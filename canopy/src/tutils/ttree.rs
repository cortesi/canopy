/*! This module defines a standard tree of instrumented nodes for testing.
 *
 *
*/
use std::cell::RefCell;

use crate::{self as canopy};
use crate::{
    event::{key, mouse},
    *,
};

/// Thread-local state tracked by test nodes.
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
    pub fn add_command(&mut self, n: &NodeName, cmd: &str) {
        self.path.push(format!("{}.{}()", n, cmd))
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

/// Get and reset the state
pub fn get_state() -> State {
    TSTATE.with(|s| s.borrow().clone())
}

pub fn state_path() -> Vec<String> {
    TSTATE.with(|s| s.borrow().path.clone())
}

macro_rules! leaf {
    ($a:ident) => {
        #[derive(Debug, PartialEq, StatefulNode)]
        pub struct $a {
            state: NodeState,

            pub next_outcome: Option<Outcome>,
        }

        impl Node for $a {
            fn accept_focus(&mut self) -> bool {
                true
            }
            fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
                r.text(
                    "any",
                    self.vp().view_rect().first_line(),
                    &format!("<{}>", self.name().clone()),
                )
            }
            fn handle_key(&mut self, _: &mut dyn Core, _: key::Key) -> Result<Outcome> {
                self.handle("key")
            }
            fn handle_mouse(&mut self, _: &mut dyn Core, _: mouse::MouseEvent) -> Result<Outcome> {
                self.handle("mouse")
            }
        }

        #[derive_commands]
        impl $a {
            pub fn new() -> Self {
                $a {
                    state: NodeState::default(),
                    next_outcome: None,
                }
            }

            #[command]
            /// A command that appears only on leaf nodes.
            pub fn c_leaf(&self, _core: &dyn Core) -> Result<()> {
                TSTATE.with(|s| {
                    s.borrow_mut().add_command(&self.name(), "c_leaf");
                });
                Ok(())
            }

            pub fn make_mouse_event(&self) -> Result<mouse::MouseEvent> {
                let a = self.vp().screen_rect();
                Ok(mouse::MouseEvent {
                    action: mouse::Action::Down,
                    button: mouse::Button::Left,
                    modifiers: key::Empty,
                    location: a.tl,
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
    };
}

macro_rules! branch {
    ($name:ident, $la:ident, $lb:ident) => {
        #[derive(Debug, PartialEq, StatefulNode)]
        pub struct $name {
            state: NodeState,

            pub next_outcome: Option<Outcome>,
            pub a: $la,
            pub b: $lb,
        }

        #[derive_commands]
        impl $name {
            pub fn new() -> Self {
                $name {
                    state: NodeState::default(),
                    a: $la::new(),
                    b: $lb::new(),
                    next_outcome: None,
                }
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

        impl Node for $name {
            fn accept_focus(&mut self) -> bool {
                true
            }

            fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
                let parts = self.vp().split_vertical(2)?;
                fit(&mut self.a, parts[0])?;
                fit(&mut self.b, parts[1])?;

                r.text(
                    "any",
                    self.vp().view_rect().first_line(),
                    &format!("<{}>", self.name().clone()),
                )
            }

            fn handle_key(&mut self, _: &mut dyn Core, _: key::Key) -> Result<Outcome> {
                self.handle("key")
            }

            fn handle_mouse(&mut self, _: &mut dyn Core, _: mouse::MouseEvent) -> Result<Outcome> {
                self.handle("mouse")
            }

            fn children(
                self: &mut Self,
                f: &mut dyn FnMut(&mut dyn Node) -> Result<()>,
            ) -> Result<()> {
                f(&mut self.a)?;
                f(&mut self.b)?;
                Ok(())
            }
        }
    };
}

#[derive(Debug, PartialEq, StatefulNode)]
pub struct R {
    state: NodeState,

    pub next_outcome: Option<Outcome>,
    pub a: Ba,
    pub b: Bb,
}

#[derive_commands]
impl R {
    pub fn new() -> Self {
        R {
            state: NodeState::default(),
            a: Ba::new(),
            b: Bb::new(),
            next_outcome: None,
        }
    }
    #[command]
    /// A command that appears only on leaf nodes.
    pub fn c_root(&self, _core: &dyn Core) -> Result<()> {
        TSTATE.with(|s| {
            s.borrow_mut().add_command(&self.name(), "c_root");
        });
        Ok(())
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

impl Node for R {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        let parts = self.vp().split_horizontal(2)?;
        fit(&mut self.a, parts[0])?;
        fit(&mut self.b, parts[1])?;

        r.text(
            "any",
            self.vp().view_rect().first_line(),
            &format!("<{}>", self.name().clone()),
        )
    }

    fn handle_key(&mut self, _: &mut dyn Core, _: key::Key) -> Result<Outcome> {
        self.handle("key")
    }

    fn handle_mouse(&mut self, _: &mut dyn Core, _: mouse::MouseEvent) -> Result<Outcome> {
        self.handle("mouse")
    }

    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.a)?;
        f(&mut self.b)?;
        Ok(())
    }
}

leaf!(BaLa);
leaf!(BaLb);
leaf!(BbLa);
leaf!(BbLb);
branch!(Ba, BaLa, BaLb);
branch!(Bb, BbLa, BbLb);
