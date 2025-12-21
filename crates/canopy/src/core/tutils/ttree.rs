/*! This module defines a standard tree of instrumented nodes for testing. */
use std::cell::RefCell;

use crate::{
    self as canopy,
    backend::test::TestRender,
    event::{key, mouse},
    geom::Expanse,
    *,
};

/// Thread-local state tracked by test nodes.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct State {
    /// Recorded event path entries.
    pub path: Vec<String>,
}

impl State {
    /// Construct a new empty state.
    pub fn new() -> Self {
        Self { path: vec![] }
    }
    /// Clear recorded events.
    pub fn reset(&mut self) {
        self.path = vec![];
    }
    /// Record a node event.
    pub fn add_event(&mut self, n: &NodeName, evt: &str, result: &EventOutcome) {
        let outcome = match result {
            EventOutcome::Handle => "handle",
            EventOutcome::Consume => "consume",
            EventOutcome::Ignore => "ignore",
        };
        self.path.push(format!("{n}@{evt}->{outcome}"))
    }
    /// Record a command invocation.
    pub fn add_command(&mut self, n: &NodeName, cmd: &str) {
        self.path.push(format!("{n}.{cmd}()"))
    }
}

thread_local! {
    pub (crate) static TSTATE: RefCell<State> = RefCell::new(State::new());
}

/// Clear the global test state.
pub fn reset_state() {
    TSTATE.with(|s| {
        s.borrow_mut().reset();
    });
}

/// Get the current test state.
pub fn get_state() -> State {
    TSTATE.with(|s| s.borrow().clone())
}

/// Build a test leaf node type.
macro_rules! leaf {
    ($a:ident) => {
        /// Test leaf node with instrumented behavior.
        #[derive(Debug, PartialEq, Eq, StatefulNode)]
        pub struct $a {
            /// Node state.
            state: NodeState,

            /// Next event outcome override.
            pub next_outcome: Option<EventOutcome>,
        }

        impl Node for $a {
            fn accept_focus(&mut self) -> bool {
                true
            }
            fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
                self.fill(sz)
            }
            fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
                r.text(
                    "any",
                    self.vp().view().line(0),
                    &format!("<{}>", self.name().clone()),
                )
            }
            fn handle_key(&mut self, _: &mut dyn Context, _: key::Key) -> Result<EventOutcome> {
                self.handle("key")
            }
            fn handle_mouse(
                &mut self,
                _: &mut dyn Context,
                _: mouse::MouseEvent,
            ) -> Result<EventOutcome> {
                self.handle("mouse")
            }
        }

        #[derive_commands]
        impl $a {
            /// Construct a new leaf node.
            pub fn new() -> Self {
                $a {
                    state: NodeState::default(),
                    next_outcome: None,
                }
            }

            #[command]
            /// A command that appears only on leaf nodes.
            pub fn c_leaf(&self, _core: &dyn Context) -> Result<()> {
                TSTATE.with(|s| {
                    s.borrow_mut().add_command(&self.name(), "c_leaf");
                });
                Ok(())
            }

            /// Build a synthetic mouse event at the node's location.
            pub fn make_mouse_event(&self) -> Result<mouse::MouseEvent> {
                let a = self.vp().screen_rect();
                Ok(mouse::MouseEvent {
                    action: mouse::Action::Down,
                    button: mouse::Button::Left,
                    modifiers: key::Empty,
                    location: a.tl,
                })
            }

            fn handle(&mut self, evt: &str) -> Result<EventOutcome> {
                let ret = if let Some(x) = self.next_outcome.clone() {
                    self.next_outcome = None;
                    x
                } else {
                    EventOutcome::Ignore
                };
                TSTATE.with(|s| {
                    s.borrow_mut().add_event(&self.name(), evt, &ret);
                });
                Ok(ret)
            }
        }
    };
}

/// Build a test branch node type.
macro_rules! branch {
    ($name:ident, $la:ident, $lb:ident) => {
        /// Test branch node with two children.
        #[derive(Debug, PartialEq, Eq, StatefulNode)]
        pub struct $name {
            /// Node state.
            state: NodeState,

            /// Next event outcome override.
            pub next_outcome: Option<EventOutcome>,
            /// Left child node.
            pub a: $la,
            /// Right child node.
            pub b: $lb,
        }

        #[derive_commands]
        impl $name {
            /// Construct a new branch node.
            pub fn new() -> Self {
                $name {
                    state: NodeState::default(),
                    a: $la::new(),
                    b: $lb::new(),
                    next_outcome: None,
                }
            }
            fn handle(&mut self, evt: &str) -> Result<EventOutcome> {
                let ret = if let Some(x) = self.next_outcome.clone() {
                    self.next_outcome = None;
                    x
                } else {
                    EventOutcome::Ignore
                };
                TSTATE.with(|s| {
                    s.borrow_mut().add_event(&self.name(), evt, &ret);
                });
                Ok(ret)
            }
        }

        impl Node for $name {
            fn accept_focus(&mut self) -> bool {
                true
            }

            fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
                self.fill(sz)?;
                let vp = self.vp();
                let parts = vp.view().split_vertical(2)?;
                l.place(&mut self.a, parts[0])?;
                l.place(&mut self.b, parts[1])?;
                Ok(())
            }

            fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
                r.text(
                    "any",
                    self.vp().view().line(0),
                    &format!("<{}>", self.name().clone()),
                )
            }

            fn handle_key(&mut self, _: &mut dyn Context, _: key::Key) -> Result<EventOutcome> {
                self.handle("key")
            }

            fn handle_mouse(
                &mut self,
                _: &mut dyn Context,
                _: mouse::MouseEvent,
            ) -> Result<EventOutcome> {
                self.handle("mouse")
            }

            fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
                f(&mut self.a)?;
                f(&mut self.b)?;
                Ok(())
            }
        }
    };
}

#[derive(Debug, PartialEq, Eq, StatefulNode)]
/// Root node for the test tree.
pub struct R {
    /// Node state.
    state: NodeState,

    /// Next event outcome override.
    pub next_outcome: Option<EventOutcome>,
    /// Left branch.
    pub a: Ba,
    /// Right branch.
    pub b: Bb,
}

#[derive_commands]
impl R {
    /// Construct a new test root.
    pub fn new() -> Self {
        Self {
            state: NodeState::default(),
            a: Ba::new(),
            b: Bb::new(),
            next_outcome: None,
        }
    }
    #[command]
    /// A command that appears only on leaf nodes.
    pub fn c_root(&self, _core: &dyn Context) -> Result<()> {
        TSTATE.with(|s| {
            s.borrow_mut().add_command(&self.name(), "c_root");
        });
        Ok(())
    }
    /// Record a test event for this node.
    fn handle(&mut self, evt: &str) -> Result<EventOutcome> {
        let ret = if let Some(x) = self.next_outcome.clone() {
            self.next_outcome = None;
            x
        } else {
            EventOutcome::Ignore
        };
        TSTATE.with(|s| {
            s.borrow_mut().add_event(&self.name(), evt, &ret);
        });
        Ok(ret)
    }
}

impl Node for R {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.fill(sz)?;
        let vp = self.vp();
        let parts = vp.view().split_horizontal(2)?;
        l.place(&mut self.a, parts[0])?;
        l.place(&mut self.b, parts[1])?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.text(
            "any",
            self.vp().view().line(0),
            &format!("<{}>", self.name()),
        )
    }

    fn handle_key(&mut self, _: &mut dyn Context, _: key::Key) -> Result<EventOutcome> {
        self.handle("key")
    }

    fn handle_mouse(&mut self, _: &mut dyn Context, _: mouse::MouseEvent) -> Result<EventOutcome> {
        self.handle("mouse")
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
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

/// Run a function on our standard dummy app built from [`ttree`]. This helper
/// is used extensively in unit tests across the codebase.
pub fn run_ttree(func: impl FnOnce(&mut Canopy, TestRender, R) -> Result<()>) -> Result<()> {
    let (_, tr) = TestRender::create();
    let mut root = R::new();
    let mut c = Canopy::new();

    c.add_commands::<R>();
    c.add_commands::<BaLa>();
    c.add_commands::<BaLb>();
    c.add_commands::<BbLa>();
    c.add_commands::<BbLb>();
    c.add_commands::<Ba>();
    c.add_commands::<Bb>();

    c.set_root_size(Expanse::new(100, 100), &mut root)?;
    reset_state();
    func(&mut c, tr, root)
}
