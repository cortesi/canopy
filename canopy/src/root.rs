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

    #[command]
    /// Focus the next node in a pre-order traversal of the app.
    fn focus_next(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_next(self)?;
        Ok(())
    }

    #[command]
    /// Focus the next node in a pre-order traversal of the app.
    fn focus_prev(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_prev(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus right.
    fn focus_right(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_right(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus left.
    fn focus_left(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_left(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus up.
    fn focus_up(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_up(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus down.
    fn focus_down(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_down(self)?;
        Ok(())
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
