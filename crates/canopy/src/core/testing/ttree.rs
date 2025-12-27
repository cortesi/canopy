/*! This module defines a standard tree of instrumented nodes for testing. */
use std::cell::RefCell;

use crate::{
    Canopy, Context, NodeId, ViewContext, command,
    core::Core,
    derive_commands,
    error::Result,
    event::Event,
    geom::Expanse,
    layout::Layout,
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

/// Generate a test leaf node type with instrumentation hooks.
macro_rules! leaf {
    ($a:ident) => {
        /// Test leaf node with instrumented behavior.
        pub struct $a {
            /// Next event outcome override.
            pub next_outcome: Option<EventOutcome>,
        }

        #[derive_commands]
        impl $a {
            /// Construct a new leaf node.
            pub fn new() -> Self {
                $a { next_outcome: None }
            }

            #[command]
            /// A command that appears only on leaf nodes.
            pub fn c_leaf(&self, _core: &mut dyn Context) -> Result<()> {
                TSTATE.with(|s| {
                    s.borrow_mut().add_command(&self.name(), "c_leaf");
                });
                Ok(())
            }
        }

        impl $a {
            fn handle(&mut self, evt: &str) -> EventOutcome {
                let ret = if let Some(x) = self.next_outcome.take() {
                    x
                } else {
                    EventOutcome::Ignore
                };
                TSTATE.with(|s| {
                    s.borrow_mut().add_event(&self.name(), evt, &ret);
                });
                ret
            }
        }

        impl Widget for $a {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
                r.text(
                    "any",
                    ctx.view().outer_rect_local().line(0),
                    &format!("<{}>", self.name()),
                )
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
                match event {
                    Event::Key(_) => self.handle("key"),
                    Event::Mouse(_) => self.handle("mouse"),
                    _ => EventOutcome::Ignore,
                }
            }

            fn name(&self) -> NodeName {
                NodeName::convert(stringify!($a))
            }
        }

        impl OutcomeTarget for $a {
            fn set_outcome(&mut self, outcome: EventOutcome) {
                self.next_outcome = Some(outcome);
            }
        }
    };
}

/// Generate a test branch node type with instrumentation hooks.
macro_rules! branch {
    ($name:ident) => {
        /// Test branch node with instrumented behavior.
        pub struct $name {
            /// Next event outcome override.
            pub next_outcome: Option<EventOutcome>,
        }

        #[derive_commands]
        impl $name {
            /// Construct a new branch node.
            pub fn new() -> Self {
                $name { next_outcome: None }
            }
        }

        impl $name {
            fn handle(&mut self, evt: &str) -> EventOutcome {
                let ret = if let Some(x) = self.next_outcome.take() {
                    x
                } else {
                    EventOutcome::Ignore
                };
                TSTATE.with(|s| {
                    s.borrow_mut().add_event(&self.name(), evt, &ret);
                });
                ret
            }
        }

        impl Widget for $name {
            fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
                true
            }

            fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
                r.text(
                    "any",
                    ctx.view().outer_rect_local().line(0),
                    &format!("<{}>", self.name()),
                )
            }

            fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
                match event {
                    Event::Key(_) => self.handle("key"),
                    Event::Mouse(_) => self.handle("mouse"),
                    _ => EventOutcome::Ignore,
                }
            }

            fn name(&self) -> NodeName {
                NodeName::convert(stringify!($name))
            }
        }

        impl OutcomeTarget for $name {
            fn set_outcome(&mut self, outcome: EventOutcome) {
                self.next_outcome = Some(outcome);
            }
        }
    };
}

leaf!(BaLa);
leaf!(BaLb);
leaf!(BbLa);
leaf!(BbLb);
branch!(Ba);
branch!(Bb);

/// Root node for the test tree.
pub struct R {
    /// Next event outcome override.
    pub next_outcome: Option<EventOutcome>,
}

#[derive_commands]
impl R {
    /// Construct a new test root.
    pub fn new() -> Self {
        Self { next_outcome: None }
    }

    #[command]
    /// A command that appears only on root.
    pub fn c_root(&self, _core: &mut dyn Context) -> Result<()> {
        TSTATE.with(|s| {
            s.borrow_mut().add_command(&self.name(), "c_root");
        });
        Ok(())
    }

    /// Handle an event and record the outcome.
    fn handle(&mut self, evt: &str) -> EventOutcome {
        let ret = if let Some(x) = self.next_outcome.take() {
            x
        } else {
            EventOutcome::Ignore
        };
        TSTATE.with(|s| {
            s.borrow_mut().add_event(&self.name(), evt, &ret);
        });
        ret
    }
}

impl OutcomeTarget for R {
    fn set_outcome(&mut self, outcome: EventOutcome) {
        self.next_outcome = Some(outcome);
    }
}

impl Widget for R {
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        true
    }

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        r.text(
            "any",
            ctx.view().outer_rect_local().line(0),
            &format!("<{}>", self.name()),
        )
    }

    fn on_event(&mut self, event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Key(_) => self.handle("key"),
            Event::Mouse(_) => self.handle("mouse"),
            _ => EventOutcome::Ignore,
        }
    }

    fn name(&self) -> NodeName {
        NodeName::convert("r")
    }
}

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
    core.set_widget(core.root, R::new());

    let a = core.add(Ba::new());
    let b = core.add(Bb::new());
    let a_a = core.add(BaLa::new());
    let a_b = core.add(BaLb::new());
    let b_a = core.add(BbLa::new());
    let b_b = core.add(BbLb::new());

    core.set_children(core.root, vec![a, b])?;
    core.set_children(a, vec![a_a, a_b])?;
    core.set_children(b, vec![b_a, b_b])?;

    core.with_layout_of(core.root, |layout| {
        *layout = Layout::row().flex_horizontal(1).flex_vertical(1);
    })?;
    core.with_layout_of(a, |layout| {
        *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
    })?;
    core.with_layout_of(b, |layout| {
        *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
    })?;
    core.with_layout_of(a_a, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(a_b, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(b_a, |layout| {
        *layout = Layout::fill();
    })?;
    core.with_layout_of(b_b, |layout| {
        *layout = Layout::fill();
    })?;

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

/// Run a function on our standard dummy app built from [`ttree`].
pub fn run_ttree(func: impl FnOnce(&mut Canopy, TestRender, TestTree) -> Result<()>) -> Result<()> {
    let (_, tr) = TestRender::create();
    let mut c = Canopy::new();

    let tree = build_tree(&mut c.core)?;

    c.add_commands::<R>();
    c.add_commands::<BaLa>();
    c.add_commands::<BaLb>();
    c.add_commands::<BbLa>();
    c.add_commands::<BbLb>();
    c.add_commands::<Ba>();
    c.add_commands::<Bb>();

    c.set_root_size(Expanse::new(100, 100))?;
    reset_state();
    func(&mut c, tr, tree)
}
