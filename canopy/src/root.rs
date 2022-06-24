use crate as canopy;
use crate::{
    state::{NodeState, StatefulNode},
    *,
};

/// A Root node that lives at the base of a Canopy app. It manages modal
/// windows, houses the Inspector and exposes a set of built-in functions.
#[derive(StatefulNode)]
pub struct Root<T>
where
    T: Node,
{
    pub app: T,
    pub state: NodeState,
}

#[derive_commands]
impl<T> Root<T>
where
    T: Node,
{
    pub fn new(app: T) -> Root<T> {
        Root {
            app,
            state: NodeState::default(),
        }
    }
    #[command]
    /// Exit from the program, restoring terminal state
    fn quit(&mut self, c: &mut dyn Core) {
        c.exit(0)
    }
}

impl<T> Node for Root<T>
where
    T: Node,
{
    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.app)
    }
    fn render(&mut self, _c: &dyn Core, _r: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.app, vp)?;
        Ok(())
    }
}
