use crate::{Context, error::Result, geom::Direction, node::Node, path::Path};

/// Minimal context implementation for tests.
pub struct DummyContext {}

impl Context for DummyContext {
    fn is_on_focus_path(&self, _n: &mut dyn Node) -> bool {
        false
    }
    fn is_focused(&self, _n: &dyn Node) -> bool {
        false
    }
    fn focus_down(&mut self, _root: &mut dyn Node) {}
    fn focus_first(&mut self, _root: &mut dyn Node) {}
    fn focus_left(&mut self, _root: &mut dyn Node) {}
    fn focus_next(&mut self, _root: &mut dyn Node) {}
    fn focus_path(&self, _root: &mut dyn Node) -> Path {
        Path::empty()
    }
    fn focus_prev(&mut self, _root: &mut dyn Node) {}
    fn focus_right(&mut self, _root: &mut dyn Node) {}
    fn focus_up(&mut self, _root: &mut dyn Node) {}
    fn needs_render(&self, n: &dyn Node) -> bool {
        !n.is_hidden()
    }
    fn set_focus(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn focus_dir(&mut self, _root: &mut dyn Node, _dir: Direction) {}
    fn scroll_to(&mut self, _n: &mut dyn Node, _x: u32, _y: u32) -> bool {
        false
    }
    fn scroll_by(&mut self, _n: &mut dyn Node, _x: i32, _y: i32) -> bool {
        false
    }
    fn page_up(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn page_down(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_up(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_down(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_left(&mut self, _n: &mut dyn Node) -> bool {
        false
    }
    fn scroll_right(&mut self, _n: &mut dyn Node) -> bool {
        false
    }

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

    fn current_focus_gen(&self) -> u64 {
        0
    }
}
