use duplicate::duplicate;

use canopy;
use canopy::{
    event::{key, mouse},
    fit_and_update,
    geom::{Rect, Size, ViewPort},
    render::term::runloop,
    style::solarized,
    widgets::{frame, list::*, InputLine, Text},
    Canopy, Node, NodeState, Outcome, Result, StatefulNode,
};

struct Handle {}

#[derive(StatefulNode)]
struct TodoItem {
    state: NodeState,
    child: Text<Handle>,
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

impl Node<Handle, ()> for TodoItem {
    fn fit(&mut self, app: &mut Canopy<Handle, ()>, target: Size) -> Result<Size> {
        self.child.fit(app, target)
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle, ()>) -> Result<()>) -> Result<()> {
        f(&self.child)
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle, ()>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        fit_and_update(app, vp.screen(), &mut self.child)?;
        if self.selected {
            app.render.style.push_layer("blue");
        }
        Ok(())
    }
}

#[derive(StatefulNode)]
struct StatusBar {
    state: NodeState,
}

impl Node<Handle, ()> for StatusBar {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        app.render.style.push_layer("statusbar");
        app.render
            .text("statusbar/text", vp.view().first_line(), "todo")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<Handle, (), List<Handle, (), TodoItem>>,
    statusbar: StatusBar,
    adder: Option<frame::Frame<Handle, (), InputLine<Handle>>>,
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

    fn open_adder(&mut self, app: &mut Canopy<Handle, ()>) -> Result<Outcome<()>> {
        let mut adder = frame::Frame::new(InputLine::new(""));
        app.set_focus(&mut adder.child)?;
        self.adder = Some(adder);
        app.taint(self);
        Ok(Outcome::handle())
    }
}

impl Node<Handle, ()> for Root {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        let a = vp.screen();
        let (ct, sb) = a.carve_vend(1);
        fit_and_update(app, sb, &mut self.statusbar)?;
        fit_and_update(app, ct, &mut self.content)?;
        if let Some(add) = &mut self.adder {
            fit_and_update(
                app,
                Rect::new(a.tl.x + 2, a.tl.y + a.h / 2, a.w - 4, 3),
                add,
            )?;
        }
        Ok(())
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn handle_mouse(
        &mut self,
        _app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        let v = &mut self.content.child.state_mut().viewport;
        match k {
            c if c == mouse::Action::ScrollDown => v.down(),
            c if c == mouse::Action::ScrollUp => v.up(),
            _ => return Ok(Outcome::ignore()),
        };
        Ok(Outcome::handle())
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
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
                    self.open_adder(app)?;
                }
                c if c == 'g' => lst.select_first(),
                c if c == 'j' || c == key::KeyCode::Down => lst.select_next(),
                c if c == 'k' || c == key::KeyCode::Up => lst.select_prev(),
                c if c == ' ' || c == key::KeyCode::PageDown => lst.page_down(),
                c if c == key::KeyCode::PageUp => lst.page_up(),
                c if c == 'q' => app.exit(0),
                _ => return Ok(Outcome::ignore()),
            };
        }
        app.taint_tree(self)?;
        Ok(Outcome::handle())
    }

    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<Handle, ()>])) -> Result<()>,
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
    colors.insert(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
    );
    let mut h = Handle {};
    let mut root = Root::new();
    runloop(colors, &mut root, &mut h)?;
    Ok(())
}
