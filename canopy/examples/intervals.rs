use std::time::Duration;

use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    layout,
    style::solarized,
    widgets::{frame, list::*, Text},
    *,
};

#[derive(StatefulNode)]
struct IntervalItem {
    state: NodeState,
    child: Text,
    selected: bool,
    value: u64,
}

#[derive_commands]
impl IntervalItem {
    fn new() -> Self {
        IntervalItem {
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
    fn poll(&mut self, c: &mut dyn Core) -> Option<Duration> {
        self.inc();
        c.taint(self);
        Some(Duration::from_secs(1))
    }

    fn fit(&mut self, target: Expanse) -> Result<()> {
        self.child.fit(target)?;
        let sz = self.child.vp().size();
        self.vp_mut().fit_size(sz, sz);
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        layout::fit(&mut self.child, vp)?;
        if self.selected {
            r.style.push_layer("blue");
        }
        Ok(())
    }
}

#[derive(StatefulNode)]
struct StatusBar {
    state: NodeState,
}

#[derive_commands]
impl StatusBar {}

impl Node for StatusBar {
    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", self.vp().view_rect().line(0), "intervals")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Intervals {
    state: NodeState,
    content: frame::Frame<List<IntervalItem>>,
    statusbar: StatusBar,
}

#[derive_commands]
impl Intervals {
    fn new() -> Self {
        Intervals {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(vec![])),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
        }
    }

    #[command]
    fn add_item(&mut self, c: &mut dyn Core) {
        let lst = &mut self.content.child;
        lst.append(IntervalItem::new());
        c.taint(self);
    }
}

impl Node for Intervals {
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let (a, b) = self.vp().carve_vend(1);
        layout::fit(&mut self.statusbar, b)?;
        layout::fit(&mut self.content, a)?;
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn children(self: &mut Self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.content)?;
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );

    cnpy.add_commands::<Root<Intervals>>();
    cnpy.add_commands::<Intervals>();
    cnpy.add_commands::<List<IntervalItem>>();

    cnpy.bind_key('a', "intervals", "intervals::add_item()")?;
    cnpy.bind_key('g', "intervals", "list::select_first()")?;
    cnpy.bind_key('j', "intervals", "list::select_next()")?;
    cnpy.bind_key('d', "intervals", "list::delete_selected()")?;
    cnpy.bind_mouse(
        mouse::Action::ScrollDown,
        "intervals",
        "list::select_next()",
    )?;
    cnpy.bind_key(key::KeyCode::Down, "intervals", "list::select_next()")?;
    cnpy.bind_key('k', "intervals", "list::select_prev()")?;
    cnpy.bind_key(key::KeyCode::Up, "intervals", "list::select_prev()")?;
    cnpy.bind_mouse(mouse::Action::ScrollUp, "intervals", "list::select_prev()")?;

    cnpy.bind_key(key::KeyCode::PageDown, "intervals", "list::page_down()")?;
    cnpy.bind_key(' ', "intervals", "list::page_down()")?;
    cnpy.bind_key(key::KeyCode::PageUp, "intervals", "list::page_up()")?;

    cnpy.bind_key('q', "intervals", "root::quit()")?;

    let root = Root::new(Intervals::new());
    runloop(cnpy, root)?;
    Ok(())
}
