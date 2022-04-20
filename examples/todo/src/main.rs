mod store;

use std::env;

use anyhow::Result;
use canopy::{
    self,
    backend::crossterm::runloop,
    event::{key, mouse},
    fit,
    geom::{Expanse, Rect},
    inspector::Inspector,
    place,
    style::solarized,
    widgets::{frame, list::*, InputLine, Text},
    BackendControl, Node, NodeState, Outcome, Render, StatefulNode,
};

#[derive(StatefulNode)]
struct TodoItem {
    state: NodeState,
    child: Text,
    selected: bool,
    todo: store::Todo,
}

impl TodoItem {
    fn new(t: store::Todo) -> Self {
        TodoItem {
            state: NodeState::default(),
            child: Text::new(&t.item),
            selected: false,
            todo: t,
        }
    }
}

impl ListItem for TodoItem {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for TodoItem {
    fn fit(&mut self, target: Expanse) -> canopy::Result<Expanse> {
        self.child.fit(target)
    }

    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node) -> canopy::Result<()>,
    ) -> canopy::Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, r: &mut Render) -> canopy::Result<()> {
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

impl Node for StatusBar {
    fn render(&mut self, r: &mut Render) -> canopy::Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", self.vp().view_rect().first_line(), "todo")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<List<TodoItem>>,
    statusbar: StatusBar,
    adder: Option<frame::Frame<InputLine>>,
}

impl Root {
    fn new() -> Result<Self> {
        let mut r = Root {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(vec![])),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
            adder: None,
        };
        r.load()?;
        Ok(r)
    }

    fn open_adder(&mut self) -> canopy::Result<Outcome> {
        let mut adder = frame::Frame::new(InputLine::new(""));
        adder.child.set_focus();
        self.adder = Some(adder);
        self.taint();
        Ok(Outcome::handle())
    }

    fn load(&mut self) -> canopy::Result<()> {
        let s = store::get().todos().unwrap();
        let todos = s.iter().map(|x| TodoItem::new(x.clone()));
        for i in todos {
            self.content.child.append(i);
        }
        Ok(())
    }
}

impl Node for Root {
    fn render(&mut self, _: &mut Render) -> canopy::Result<()> {
        let vp = self.vp();
        let (a, b) = vp.carve_vend(1);
        fit(&mut self.statusbar, b)?;
        fit(&mut self.content, a)?;

        let a = vp.screen_rect();
        if let Some(add) = &mut self.adder {
            place(add, Rect::new(a.tl.x + 2, a.tl.y + a.h / 2, a.w - 4, 3))?;
        }
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn handle_mouse(
        &mut self,
        _: &mut dyn BackendControl,
        k: mouse::Mouse,
    ) -> canopy::Result<Outcome> {
        let v = &mut self.content.child;
        match k {
            c if c == mouse::MouseAction::ScrollDown => v.update_viewport(&|vp| vp.down()),
            c if c == mouse::MouseAction::ScrollUp => v.update_viewport(&|vp| vp.up()),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn handle_key(
        &mut self,
        ctrl: &mut dyn BackendControl,
        k: key::Key,
    ) -> canopy::Result<Outcome> {
        let lst = &mut self.content.child;
        if let Some(adder) = &mut self.adder {
            match k {
                c if c == key::KeyCode::Enter => {
                    let item = store::get().add_todo(&adder.child.text()).unwrap();
                    lst.append(TodoItem::new(item));
                    self.adder = None;
                }
                c if c == key::KeyCode::Esc => {
                    self.adder = None;
                }
                _ => return Ok(Outcome::ignore()),
            };
        } else {
            match k {
                c if c == 'a' => {
                    self.open_adder()?;
                }
                c if c == 'g' => lst.select_first(),
                c if c == 'j' || c == key::KeyCode::Down => lst.select_next(),
                c if c == 'k' || c == key::KeyCode::Up => lst.select_prev(),
                c if c == ' ' || c == key::KeyCode::PageDown => lst.page_down(),
                c if c == key::KeyCode::PageUp => lst.page_up(),
                c if c == 'q' => ctrl.exit(0),
                _ => return Ok(Outcome::ignore()),
            };
        }
        canopy::taint_tree(self);
        Ok(Outcome::handle())
    }

    fn children(
        self: &mut Self,
        f: &mut dyn FnMut(&mut dyn Node) -> canopy::Result<()>,
    ) -> canopy::Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.content)?;
        if let Some(a) = &mut self.adder {
            f(a)?;
        }
        Ok(())
    }
}

pub fn main() -> Result<()> {
    if let Some(path) = env::args().nth(1) {
        store::open(&path)?;
        let mut colors = solarized::solarized_dark();
        colors.add(
            "statusbar/text",
            Some(solarized::BASE02),
            Some(solarized::BASE1),
            None,
        );
        let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new()?);
        // let mut root = Root::new();
        runloop(&mut colors, &mut root)?;
    } else {
        println!("Specify a file path")
    }

    Ok(())
}
