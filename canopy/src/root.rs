use crate as canopy;
use crate::{
    event::key::*,
    inspector::Inspector,
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
    app: T,
    state: NodeState,
    inspector: Inspector,
    inspector_active: bool,
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
            inspector: Inspector::new(),
            inspector_active: false,
        }
    }

    /// Start with the inspector open.
    pub fn with_inspector(mut self, state: bool) -> Self {
        self.inspector_active = state;
        if state {
            self.inspector.unhide();
        }
        self
    }

    #[command]
    /// Exit from the program, restoring terminal state. If the inspector is
    /// open, exit the inspector instead.
    pub fn quit(&mut self, c: &mut dyn Core) -> Result<()> {
        if self.inspector_active {
            self.hide_inspector(c)?;
        } else {
            c.exit(0)
        }
        Ok(())
    }

    #[command]
    /// Focus the next node in a pre-order traversal of the app.
    pub fn focus_next(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_next(self)?;
        Ok(())
    }

    #[command]
    /// Focus the next node in a pre-order traversal of the app.
    pub fn focus_prev(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_prev(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus right.
    pub fn focus_right(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_right(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus left.
    pub fn focus_left(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_left(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus up.
    pub fn focus_up(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_up(self)?;
        Ok(())
    }

    #[command]
    /// Shift focus down.
    pub fn focus_down(&mut self, c: &mut dyn Core) -> Result<()> {
        c.focus_down(self)?;
        Ok(())
    }

    #[command]
    /// Hide the inspector.
    pub fn hide_inspector(&mut self, c: &mut dyn Core) -> Result<()> {
        self.inspector_active = false;
        self.inspector.hide();
        c.taint_tree(self);
        c.focus_first(&mut self.app)?;
        Ok(())
    }

    #[command]
    /// Show the inspector.
    pub fn activate_inspector(&mut self, c: &mut dyn Core) -> Result<()> {
        self.inspector_active = true;
        self.inspector.unhide();
        c.taint_tree(self);
        c.focus_first(&mut self.inspector)?;
        Ok(())
    }

    #[command]
    /// Show the inspector.
    pub fn toggle_inspector(&mut self, c: &mut dyn Core) -> Result<()> {
        if self.inspector_active {
            self.hide_inspector(c)
        } else {
            self.activate_inspector(c)
        }
    }

    #[command]
    /// If we're currently focused in the inspector, shift focus into the app pane instead.
    pub fn focus_app(&mut self, c: &mut dyn Core) -> Result<()> {
        if c.is_on_focus_path(&mut self.inspector) {
            c.focus_first(&mut self.app)?;
        }
        Ok(())
    }
}

impl<T> Node for Root<T>
where
    T: Node,
{
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.inspector)?;
        f(&mut self.app)
    }
    fn render(&mut self, _c: &dyn Core, _r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if self.inspector_active {
            let parts = vp.split_horizontal(2)?;
            fit(&mut self.inspector, parts[0])?;
            fit(&mut self.app, parts[1])?;
        } else {
            fit(&mut self.app, vp)?;
        };

        Ok(())
    }
}

impl<T> DefaultBindings for Root<T>
where
    T: Node,
{
    fn defaults(b: Binder) -> Binder {
        b.defaults::<Inspector>()
            .with_path("root")
            .key(Ctrl + KeyCode::Right, "root::toggle_inspector()")
            .key('q', "root::quit()")
            .with_path("inspector")
            .key('a', "root::focus_app()")
    }
}

impl<T> Loader for Root<T>
where
    T: Loader + Node,
{
    fn load(c: &mut Canopy) {
        c.add_commands::<Root<T>>();
        T::load(c);
        Inspector::load(c);
    }
}
