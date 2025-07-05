use canopy::{
    derive_commands,
    event::{key, mouse},
    widgets::{frame, Text},
    *,
};

#[derive(StatefulNode)]
pub struct Pager {
    state: NodeState,
    child: frame::Frame<Text>,
}

#[derive_commands]
impl Pager {
    pub fn new(contents: String) -> Self {
        Pager {
            state: NodeState::default(),
            child: frame::Frame::new(Text::new(&contents)),
        }
    }
}

impl Node for Pager {
    fn accept_focus(&mut self) -> bool {
        true
    }

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

impl Loader for Pager {
    fn load(c: &mut Canopy) {
        c.add_commands::<Text>();
    }
}

pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.bind_key('g', "pager", "text::scroll_to_top()")
        .unwrap();

    cnpy.bind_key('j', "pager", "text::scroll_down()").unwrap();
    cnpy.bind_key(key::KeyCode::Down, "pager", "text::scroll_down()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollDown, "pager", "text::scroll_down()")
        .unwrap();
    cnpy.bind_key('k', "pager", "text::scroll_up()").unwrap();
    cnpy.bind_key(key::KeyCode::Up, "pager", "text::scroll_up()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollUp, "pager", "text::scroll_up()")
        .unwrap();

    cnpy.bind_key('h', "pager", "text::scroll_left()").unwrap();
    cnpy.bind_key(key::KeyCode::Left, "pager", "text::scroll_left()")
        .unwrap();
    cnpy.bind_key('l', "pager", "text::scroll_right()").unwrap();
    cnpy.bind_key(key::KeyCode::Right, "pager", "text::scroll_right()")
        .unwrap();

    cnpy.bind_key(key::KeyCode::PageDown, "pager", "text::page_down()")
        .unwrap();
    cnpy.bind_key(' ', "pager", "text::page_down()").unwrap();
    cnpy.bind_key(key::KeyCode::PageUp, "pager", "text::page_up()")
        .unwrap();

    cnpy.bind_key('q', "root", "root::quit()").unwrap();
}
