/*! This module defines a standard tree of instrumented nodes for testing. */
use std::cell::RefCell;

use crate::{
    Canopy, Context, NodeId, ViewContext, command,
    core::Core,
    derive_commands,
    error::Result,
    event::Event,
    geom::Size,
    layout::{Direction, Layout},
    render::Render,
    state::NodeName,
    testing::backend::TestRender,
    widget::{EventOutcome, Widget},
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
    pub(crate) static TSTATE: RefCell<State> = RefCell::new(State::new());
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

/// Allows tests to set the next event outcome on a node.
pub trait OutcomeTarget {
    /// Set the next event outcome.
    fn set_outcome(&mut self, outcome: EventOutcome);
}

/// Define an instrumented test node.
///
/// `node!(Type)` names the node after its type; `node!(Type, "name")` overrides that name. A
/// trailing identifier adds a `#[command]` method of that name which records its own call.
macro_rules! node {
    ($type:ident) => {
        node!(@node $type, stringify!($type), {});
    };
    ($type:ident, $name:literal) => {
        node!(@node $type, $name, {});
    };
    ($type:ident, $command:ident) => {
        node!(@node $type, stringify!($type), {
            #[command]
            /// A command that only this node kind carries.
            pub fn $command(&self, _core: &mut dyn Context) -> Result<()> {
                TSTATE.with(|s| {
                    s.borrow_mut().add_command(&self.name(), stringify!($command));
                });
                Ok(())
            }
        });
    };
    ($type:ident, $name:literal, $command:ident) => {
        node!(@node $type, $name, {
            #[command]
            /// A command that only this node kind carries.
            pub fn $command(&self, _core: &mut dyn Context) -> Result<()> {
                TSTATE.with(|s| {
                    s.borrow_mut().add_command(&self.name(), stringify!($command));
                });
                Ok(())
            }
        });
    };
    (@node $type:ident, $name:expr, { $($command:tt)* }) => {
        /// Test node with instrumented event and command behavior.
        pub struct $type {
            /// Next event outcome override.
            pub next_outcome: Option<EventOutcome>,
        }

        #[derive_commands]
        impl $type {
            /// Construct a new test node.
            pub fn new() -> Self {
                Self { next_outcome: None }
            }

            $($command)*
        }

        impl $type {
            /// Record an event and return the pending outcome override.
            fn handle(&mut self, evt: &str) -> EventOutcome {
                let ret = self.next_outcome.take().unwrap_or(EventOutcome::Ignore);
                TSTATE.with(|s| {
                    s.borrow_mut().add_event(&self.name(), evt, &ret);
                });
                ret
            }
        }

        impl Widget for $type {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
                r.text(
                    "any",
                    ctx.view().outer_rect_local().line(0)?,
                    &format!("<{}>", self.name()),
                )
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> Result<EventOutcome> {
                let outcome = match event {
                    Event::Key(_) => self.handle("key"),
                    Event::Mouse(_) => self.handle("mouse"),
                    _ => EventOutcome::Ignore,
                };
                Ok(outcome)
            }

            fn name(&self) -> NodeName {
                NodeName::convert($name)
            }
        }

        impl OutcomeTarget for $type {
            fn set_outcome(&mut self, outcome: EventOutcome) {
                self.next_outcome = Some(outcome);
            }
        }
    };
}

node!(BaLa, c_leaf);
node!(BaLb, c_leaf);
node!(BbLa, c_leaf);
node!(BbLb, c_leaf);
node!(Ba);
node!(Bb);
node!(R, "r", c_root);

/// Node IDs for the test tree.
#[derive(Debug, Clone, Copy)]
pub struct TestTree {
    /// Root node id.
    pub root: NodeId,
    /// Left branch node id.
    pub a: NodeId,
    /// Right branch node id.
    pub b: NodeId,
    /// Left-left leaf id.
    pub a_a: NodeId,
    /// Left-right leaf id.
    pub a_b: NodeId,
    /// Right-left leaf id.
    pub b_a: NodeId,
    /// Right-right leaf id.
    pub b_b: NodeId,
}

/// Build the standard test tree and attach layout styles.
fn build_tree(core: &mut Core) -> Result<TestTree> {
    core.replace_subtree(core.root, R::new())?;

    let a = core.create_detached(Ba::new())?;
    let b = core.create_detached(Bb::new())?;
    let a_a = core.create_detached(BaLa::new())?;
    let a_b = core.create_detached(BaLb::new())?;
    let b_a = core.create_detached(BbLa::new())?;
    let b_b = core.create_detached(BbLb::new())?;

    core.set_children(core.root, vec![a, b])?;
    core.set_children(a, vec![a_a, a_b])?;
    core.set_children(b, vec![b_a, b_b])?;

    core.set_layout_of(core.root, Layout::fill().direction(Direction::Row))?;
    core.set_layout_of(a, Layout::fill())?;
    core.set_layout_of(b, Layout::fill())?;
    core.set_layout_of(a_a, Layout::fill())?;
    core.set_layout_of(a_b, Layout::fill())?;
    core.set_layout_of(b_a, Layout::fill())?;
    core.set_layout_of(b_b, Layout::fill())?;

    Ok(TestTree {
        root: core.root,
        a,
        b,
        a_a,
        a_b,
        b_a,
        b_b,
    })
}

/// Run a function on our standard dummy app built from [`TestTree`].
pub fn run_ttree(func: impl FnOnce(&mut Canopy, TestRender, TestTree) -> Result<()>) -> Result<()> {
    let tr = TestRender::new();
    let mut c = Canopy::new();

    let tree = build_tree(&mut c.core)?;

    c.add_commands::<R>()?;
    c.add_commands::<BaLa>()?;
    c.add_commands::<BaLb>()?;
    c.add_commands::<BbLa>()?;
    c.add_commands::<BbLb>()?;
    c.add_commands::<Ba>()?;
    c.add_commands::<Bb>()?;

    c.set_root_size(Size::new(100, 100))?;
    reset_state();
    func(&mut c, tr, tree)
}
