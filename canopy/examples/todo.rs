use duplicate::duplicate_item;

use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    geom::{Rect, Size},
    inspector::Inspector,
    style::solarized,
    widgets::{frame, list::*, InputLine, Text},
    BackendControl, Node, NodeState, Outcome, Render, Result, StatefulNode, ViewPort,
};

#[derive(StatefulNode)]
struct TodoItem {
    state: NodeState,
    child: Text,
    selected: bool,
}

impl TodoItem {
    fn new(text: &str) -> Self {
        TodoItem {
            state: NodeState::default(),
            child: Text::new(text),
            selected: false,
        }
    }
}

impl ListItem for TodoItem {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for TodoItem {
    fn fit(&mut self, target: Size) -> Result<Size> {
        self.child.fit(target)
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node) -> Result<()>) -> Result<()> {
        f(&self.child)
    }

    fn children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        self.child.wrap(vp)?;
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
    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", vp.view_rect().first_line(), "todo")?;
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
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(vec![])),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
            adder: None,
        }
    }

    fn open_adder(&mut self) -> Result<Outcome> {
        let mut adder = frame::Frame::new(InputLine::new(""));
        adder.child.handle_focus()?;
        self.adder = Some(adder);
        self.taint();
        Ok(Outcome::handle())
    }
}

impl Node for Root {
    fn render(&mut self, _: &mut Render, vp: ViewPort) -> Result<()> {
        let (a, b) = vp.carve_vend(1);
        self.statusbar.wrap(b)?;
        self.content.wrap(a)?;

        let a = vp.screen_rect();
        if let Some(add) = &mut self.adder {
            add.place(Rect::new(a.tl.x + 2, a.tl.y + a.h / 2, a.w - 4, 3))?;
        }
        Ok(())
    }

    fn handle_focus(&mut self) -> Result<Outcome> {
        self.set_focus();
        Ok(Outcome::handle())
    }

    fn handle_mouse(&mut self, _: &mut dyn BackendControl, k: mouse::Mouse) -> Result<Outcome> {
        let v = &mut self.content.child;
        match k {
            c if c == mouse::MouseAction::ScrollDown => v.update_viewport(&|vp| vp.down()),
            c if c == mouse::MouseAction::ScrollUp => v.update_viewport(&|vp| vp.up()),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn handle_key(&mut self, ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        let lst = &mut self.content.child;
        if let Some(adder) = &mut self.adder {
            match k {
                c if c == key::KeyCode::Enter => {
                    lst.append(TodoItem::new(&adder.child.text()));
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
        canopy::taint_tree(self)?;
        Ok(Outcome::handle())
    }

    #[duplicate_item(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node])) -> Result<()>,
    ) -> Result<()> {
        f(reference([self.statusbar]))?;
        f(reference([self.content]))?;
        if let Some(a) = reference([self.adder]) {
            f(a)?;
        }
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut colors = solarized::solarized_dark();
    colors.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );
    let mut root = Inspector::new(key::Ctrl + key::KeyCode::Right, Root::new());
    runloop(colors, &mut root)?;
    Ok(())
}
