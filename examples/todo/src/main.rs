use anyhow::Result;
use clap::Parser;

use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    geom::{Expanse, Rect},
    style::solarized,
    widgets::{frame, list::*, Input, Text},
    *,
};

mod store;

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

#[derive_commands]
impl Node for TodoItem {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> canopy::Result<()> {
        self.child.layout(l, sz)?;
        let vp = self.child.vp();
        l.wrap(&mut self.child, vp)?;
        Ok(())
    }

    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node) -> canopy::Result<()>,
    ) -> canopy::Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> canopy::Result<()> {
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
    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> canopy::Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", self.vp().view().line(0), "todo")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Todo {
    state: NodeState,
    content: frame::Frame<List<TodoItem>>,
    statusbar: StatusBar,
    adder: Option<frame::Frame<Input>>,
}

#[derive_commands]
impl Todo {
    fn new() -> Result<Self> {
        let mut r = Todo {
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

    /// Open the editor to enter a new todo item.
    #[command]
    fn enter_item(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        let mut adder = frame::Frame::new(Input::new(""));
        c.set_focus(&mut adder.child);
        self.adder = Some(adder);
        c.taint(self);
        Ok(())
    }

    /// Open the editor to enter a new todo item.
    #[command]
    fn delete_item(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        let lst = &mut self.content.child;
        if let Some(t) = lst.selected() {
            store::get().delete_todo(t.todo.id).unwrap();
            lst.delete_selected(c);
        }
        Ok(())
    }

    /// Accept the new item we're currently editing.
    #[command]
    fn accept_add(&mut self, _: &mut dyn Context) -> canopy::Result<()> {
        if let Some(adder) = &mut self.adder {
            let item = store::get().add_todo(&adder.child.text()).unwrap();
            self.content.child.append(TodoItem::new(item));
            self.adder = None;
        }
        Ok(())
    }

    /// Close the add item editor.
    #[command]
    fn cancel_add(&mut self, _: &mut dyn Context) -> canopy::Result<()> {
        self.adder = None;
        Ok(())
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

impl Node for Todo {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> canopy::Result<()> {
        l.fill(self, sz)?;
        let vp = self.vp();
        let (a, b) = vp.view().carve_vend(1);
        l.place(&mut self.statusbar, vp, b)?;
        l.place(&mut self.content, vp, a)?;

        let a = self.vp().screen_rect();
        if let Some(add) = &mut self.adder {
            let w = a.w.saturating_sub(4);
            l.place(add, vp, Rect::new(a.tl.x + 2, a.tl.y + a.h / 2, w, 3))?;
        }
        Ok(())
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn children(
        &mut self,
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

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of times to greet
    #[clap(short, long)]
    commands: bool,

    path: Option<String>,
}

pub fn main() -> Result<()> {
    let args = Args::parse();

    let mut cnpy = Canopy::new();
    cnpy.add_commands::<List<TodoItem>>();
    cnpy.add_commands::<Todo>();
    cnpy.add_commands::<Input>();

    canopy::Binder::new(&mut cnpy)
        .with_path("todo/")
        .key('q', "root::quit()")
        .key('d', "todo::delete_item()")
        .key('a', "todo::enter_item()")
        .key('g', "list::select_first()")
        .key('j', "list::select_next()")
        .key(key::KeyCode::Down, "list::select_next()")
        .key('k', "list::select_prev()")
        .key(key::KeyCode::Up, "list::select_prev()")
        .key(' ', "list::page_down()")
        .key(key::KeyCode::PageDown, "list::page_down()")
        .key(key::KeyCode::PageUp, "list::page_up()")
        .mouse(mouse::Action::ScrollUp, "list::select_prev()")
        .mouse(mouse::Action::ScrollDown, "list::select_next()")
        .with_path("input")
        .defaults::<Input>()
        .key(key::KeyCode::Enter, "todo::accept_add()")
        .key(key::KeyCode::Esc, "todo::cancel_add()");

    if args.commands {
        cnpy.print_command_table(&mut std::io::stdout())?;
        return Ok(());
    }

    if let Some(path) = args.path {
        store::open(&path)?;

        cnpy.style.add(
            "statusbar/text",
            Some(solarized::BASE02),
            Some(solarized::BASE1),
            None,
        );

        runloop(cnpy, Todo::new()?)?;
    } else {
        println!("Specify a file path")
    }

    Ok(())
}

