pub mod ttree;
pub use ttree::*;

use crate::{self as canopy};
use crate::{
    backend::test::TestRender,
    geom::{Direction, Expanse, Rect},
    path::Path,
    widgets::list::ListItem,
    *,
};

// A fixed-size test node
#[derive(Debug, PartialEq, Eq, StatefulNode)]
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

pub fn run(func: impl FnOnce(&mut Canopy, TestRender, ttree::R) -> Result<()>) -> Result<()> {
    let (_, tr) = TestRender::create();
    let mut root = ttree::R::new();
    let mut c = Canopy::new();

    c.add_commands::<ttree::R>();
    c.add_commands::<ttree::BaLa>();
    c.add_commands::<ttree::BaLb>();
    c.add_commands::<ttree::BbLa>();
    c.add_commands::<ttree::BbLb>();
    c.add_commands::<ttree::Ba>();
    c.add_commands::<ttree::Bb>();

    c.set_root_size(Expanse::new(100, 100), &mut root)?;
    ttree::reset_state();
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

    /// Start the backend renderer.
    fn start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the backend renderer, releasing control of the terminal.
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the render backend and exit the process.
    fn exit(&mut self, _code: i32) -> ! {
        panic!("exit in dummy core")
    }
}
