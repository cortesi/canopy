use canopy::{
    derive_commands,
    event::key,
    geom::Expanse,
    widgets::{frame, Editor},
    *,
};

#[derive(StatefulNode)]
pub struct Ed {
    state: NodeState,
    child: frame::Frame<Editor>,
}

#[derive_commands]
impl Ed {
    pub fn new(contents: String) -> Self {
        Ed {
            state: NodeState::default(),
            child: frame::Frame::new(Editor::new(&contents)),
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
        c.add_commands::<Ed>();
        c.add_commands::<Editor>();
    }
}

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
