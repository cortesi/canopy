use std::time::Duration;

use canopy::{
    backend::crossterm::runloop,
    derive_commands,
    event::{key, mouse},
    geom::Expanse,
    inspector::Inspector,
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

    fn fit(&mut self, target: Expanse) -> Result<Expanse> {
        self.child.fit(target)
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.child, vp)?;
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
        r.text(
            "statusbar/text",
            self.vp().view_rect().first_line(),
            "intervals",
        )?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<List<IntervalItem>>,
    statusbar: StatusBar,
}

#[derive_commands]
impl Root {
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(vec![])),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
        }
    }

    fn add_item(&mut self, c: &mut dyn Core) -> Result<Outcome> {
        let lst = &mut self.content.child;
        lst.append(IntervalItem::new());
        c.taint(self);
        Ok(Outcome::Handle)
    }
}

impl Node for Root {
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let (a, b) = self.vp().carve_vend(1);
        fit(&mut self.statusbar, b)?;
        fit(&mut self.content, a)?;
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn handle_mouse(&mut self, _c: &mut dyn Core, k: mouse::MouseEvent) -> Result<Outcome> {
        let v = &mut self.content.child;
        match k {
            c if c == mouse::MouseAction::ScrollDown => v.update_viewport(&|vp| vp.down()),
            c if c == mouse::MouseAction::ScrollUp => v.update_viewport(&|vp| vp.up()),
            _ => return Ok(Outcome::Ignore),
        };
        Ok(Outcome::Handle)
    }

    fn handle_key(&mut self, c: &mut dyn Core, k: key::Key) -> Result<Outcome> {
        let lst = &mut self.content.child;
        match k {
            ck if ck == 'a' => {
                self.add_item(c)?;
            }
            ck if ck == 'd' => {
                lst.delete_selected(c);
            }
            ck if ck == 'g' => lst.select_first(c),
            ck if ck == 'j' || ck == key::KeyCode::Down => lst.select_next(c),
            ck if ck == 'k' || ck == key::KeyCode::Up => lst.select_prev(c),
            ck if ck == ' ' || ck == key::KeyCode::PageDown => lst.page_down(c),
            ck if ck == key::KeyCode::PageUp => lst.page_up(c),
            ck if ck == 'q' => c.exit(0),
            _ => return Ok(Outcome::Ignore),
        };
        c.taint_tree(self);
        Ok(Outcome::Handle)
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
    let root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new());
    runloop(cnpy, root)?;
    Ok(())
}
