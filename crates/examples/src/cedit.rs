use canopy::{
    Binder, Canopy, Layout, Loader, derive_commands,
    error::Result,
    event::key,
    geom::Expanse,
    node::Node,
    state::{NodeState, StatefulNode},
    widgets::{Root, editor::Editor, frame},
};

#[derive(canopy::StatefulNode)]
/// Simple editor wrapper for the cedit demo.
pub struct Ed {
    /// Node state.
    state: NodeState,
    /// Wrapped editor widget.
    child: frame::Frame<Editor>,
}

#[derive_commands]
impl Ed {
    /// Construct an editor with initial contents.
    pub fn new(contents: &str) -> Self {
        Self {
            state: NodeState::default(),
            child: frame::Frame::new(Editor::new(contents)),
        }
    }
}

impl Node for Ed {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.child.layout(l, sz)?;
        let vp = self.child.vp();
        self.wrap(vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

impl Loader for Ed {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<Editor>();
    }
}

/// Install key bindings for the cedit demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .defaults::<Root<Ed>>()
        .with_path("ed/")
        .key(key::KeyCode::Left, "editor::cursor_shift(1)")
        .key(key::KeyCode::Right, "editor::cursor_shift(-1)")
        .key(key::KeyCode::Down, "editor::cursor_shift_lines(1)")
        .key(key::KeyCode::Up, "editor::cursor_shift_lines(-1)")
        .key('h', "editor::cursor_shift(-1)")
        .key('l', "editor::cursor_shift(1)")
        .key('j', "editor::cursor_shift_chunk(1)")
        .key('k', "editor::cursor_shift_chunk(-1)")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key('p', "print(\"xxxx\")");
}
