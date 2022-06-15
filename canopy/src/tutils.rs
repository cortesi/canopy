use std::cell::RefCell;

use crate::{self as canopy, BackendControl};
use crate::{
    backend::test::TestRender,
    event::{key, mouse},
    geom::{Direction, Expanse, Rect},
    path::Path,
    widgets::list::ListItem,
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

pub fn get_state() -> State {
    TSTATE.with(|s| s.borrow().clone())
}

pub fn state_path() -> Vec<String> {
    TSTATE.with(|s| s.borrow().path.clone())
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

impl Node for TLeaf {
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
    fn handle_key(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: key::Key,
    ) -> Result<Outcome> {
        self.handle("key")
    }
    fn handle_mouse(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: mouse::Mouse,
    ) -> Result<Outcome> {
        self.handle("mouse")
    }
}

impl Node for TBranch {
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

    fn handle_key(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: key::Key,
    ) -> Result<Outcome> {
        self.handle("key")
    }

    fn handle_mouse(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: mouse::Mouse,
    ) -> Result<Outcome> {
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

    fn handle_key(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: key::Key,
    ) -> Result<Outcome> {
        self.handle("key")
    }

    fn handle_mouse(
        &mut self,
        _: &mut dyn Core,
        _: &mut dyn BackendControl,
        _: mouse::Mouse,
    ) -> Result<Outcome> {
        self.handle("mouse")
    }

    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.a)?;
        f(&mut self.b)?;
        Ok(())
    }
}

#[derive_commands]
impl TLeaf {
    pub fn new(name: &str) -> Self {
        let mut n = TLeaf {
            state: NodeState::default(),
            next_outcome: None,
        };
        n.set_name(name.try_into().unwrap());
        n
    }

    #[command]
    /// A command that appears only on leaf nodes.
    pub fn c_leaf(&self, _core: &dyn Core) -> Result<()> {
        TSTATE.with(|s| {
            s.borrow_mut().add_command(&self.name(), "c_leaf");
        });
        Ok(())
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

pub fn run(func: impl FnOnce(&mut Canopy, TestRender, TRoot) -> Result<()>) -> Result<()> {
    let (_, tr) = TestRender::create();
    let mut root = TRoot::new();
    let mut c = Canopy::new();

    c.load_commands_as::<TRoot>("r")?;
    c.load_commands_as::<TLeaf>("ba_la")?;
    c.load_commands_as::<TLeaf>("ba_lb")?;
    c.load_commands_as::<TLeaf>("bb_la")?;
    c.load_commands_as::<TLeaf>("bb_lb")?;
    c.load_commands_as::<TBranch>("ba")?;
    c.load_commands_as::<TBranch>("bb")?;

    c.set_root_size(Expanse::new(100, 100), &mut root)?;
    reset_state();
    func(&mut c, tr, root)
}

pub struct DummyCore {}

impl Core for DummyCore {
    fn is_on_focus_path(&self, _n: &mut dyn Node) -> bool {
        false
    }
    fn is_focused(&self, _n: &dyn Node) -> bool {
        false
    }
    fn is_focus_ancestor(&self, _n: &mut dyn Node) -> bool {
        false
    }
    fn focus_area(&self, _root: &mut dyn Node) -> Option<Rect> {
        None
    }
    fn focus_depth(&self, _n: &mut dyn Node) -> usize {
        0
    }
    fn focus_down(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn focus_first(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }

    fn focus_left(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn focus_next(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn focus_path(&self, _root: &mut dyn Node) -> Path {
        Path::empty()
    }
    fn focus_prev(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn focus_right(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn focus_up(&mut self, _root: &mut dyn Node) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn needs_render(&self, _n: &dyn Node) -> bool {
        false
    }
    fn set_focus(&mut self, _n: &mut dyn Node) {}
    fn shift_focus(&mut self, _root: &mut dyn Node, _dir: Direction) -> Result<Outcome> {
        Ok(Outcome::Handle)
    }
    fn taint(&mut self, _n: &mut dyn Node) {}
    fn taint_tree(&mut self, _e: &mut dyn Node) {}
}
