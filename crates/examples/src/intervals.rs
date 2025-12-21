use std::time::Duration;

use canopy::{
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    style::solarized,
    widgets::{frame, list::*, Text},
    *,
};

#[derive(StatefulNode)]
pub struct IntervalItem {
    state: NodeState,
    child: Text,
    selected: bool,
    value: u64,
}

#[derive_commands]
impl Default for IntervalItem {
    fn default() -> Self {
        Self::new()
    }
}

impl IntervalItem {
    pub fn new() -> Self {
        Self {
            state: NodeState::default(),
            child: Text::new("0"),
            selected: false,
            value: 0,
        }
    }
    fn inc(&mut self) {
        self.value += 1;
        self.child = Text::new(&format!("{}", self.value))
    }
}

impl ListItem for IntervalItem {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for IntervalItem {
    fn poll(&mut self, _c: &mut dyn Context) -> Option<Duration> {
        self.inc();
        Some(Duration::from_secs(1))
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

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        if self.selected {
            r.style.push_layer("blue");
        }
        Ok(())
    }
}

#[derive(StatefulNode)]
pub struct StatusBar {
    state: NodeState,
}

#[derive_commands]
impl StatusBar {}

impl Node for StatusBar {
    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", self.vp().view().line(0), "intervals")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
pub struct Intervals {
    state: NodeState,
    content: frame::Frame<List<IntervalItem>>,
    statusbar: StatusBar,
}

#[derive_commands]
impl Default for Intervals {
    fn default() -> Self {
        Self::new()
    }
}

impl Intervals {
    pub fn new() -> Self {
        Self {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(vec![])),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
        }
    }

    #[command]
    pub fn add_item(&mut self, _c: &mut dyn Context) {
        let lst = &mut self.content.child;
        lst.append(IntervalItem::new());
    }
}

impl Node for Intervals {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        self.fill(sz)?;
        let vp = self.vp();
        let (a, b) = vp.view().carve_vend(1);
        l.place(&mut self.statusbar, b)?;
        l.place(&mut self.content, a)?;
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.content)?;
        Ok(())
    }
}

impl Loader for Intervals {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<List<IntervalItem>>();
    }
}

pub fn setup_bindings(cnpy: &mut Canopy) {
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );

    cnpy.bind_key('a', "intervals", "intervals::add_item()")
        .unwrap();
    cnpy.bind_key('g', "intervals", "list::select_first()")
        .unwrap();
    cnpy.bind_key('j', "intervals", "list::select_next()")
        .unwrap();
    cnpy.bind_key('d', "intervals", "list::delete_selected()")
        .unwrap();
    cnpy.bind_mouse(
        mouse::Action::ScrollDown,
        "intervals",
        "list::select_next()",
    )
    .unwrap();
    cnpy.bind_key(key::KeyCode::Down, "intervals", "list::select_next()")
        .unwrap();
    cnpy.bind_key('k', "intervals", "list::select_prev()")
        .unwrap();
    cnpy.bind_key(key::KeyCode::Up, "intervals", "list::select_prev()")
        .unwrap();
    cnpy.bind_mouse(mouse::Action::ScrollUp, "intervals", "list::select_prev()")
        .unwrap();

    cnpy.bind_key(key::KeyCode::PageDown, "intervals", "list::page_down()")
        .unwrap();
    cnpy.bind_key(' ', "intervals", "list::page_down()")
        .unwrap();
    cnpy.bind_key(key::KeyCode::PageUp, "intervals", "list::page_up()")
        .unwrap();

    cnpy.bind_key('q', "intervals", "root::quit()").unwrap();
}
