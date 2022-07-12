use anyhow::Result;
use clap::Parser;

use canopy::{
    backend::crossterm::runloop,
    event::{key, mouse},
    geom::{Expanse, Rect},
    inspector::Inspector,
    style::solarized,
    widgets::{frame, list::*, InputLine, Text},
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
    fn fit(&mut self, target: Expanse) -> canopy::Result<Expanse> {
        self.child.fit(target)
    }

    fn children(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node) -> canopy::Result<()>,
    ) -> canopy::Result<()> {
        f(&mut self.child)
    }

    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> canopy::Result<()> {
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
    fn render(&mut self, _c: &dyn Core, r: &mut Render) -> canopy::Result<()> {
        r.style.push_layer("statusbar");
        r.text("statusbar/text", self.vp().view_rect().first_line(), "todo")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Todo {
    state: NodeState,
    content: frame::Frame<List<TodoItem>>,
    statusbar: StatusBar,
    adder: Option<frame::Frame<InputLine>>,
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
    fn enter_item(&mut self, c: &mut dyn Core) -> canopy::Result<()> {
        let mut adder = frame::Frame::new(InputLine::new(""));
        c.set_focus(&mut adder.child);
        self.adder = Some(adder);
        c.taint(self);
        Ok(())
    }

    /// Open the editor to enter a new todo item.
    #[command]
    fn delete_item(&mut self, c: &mut dyn Core) -> canopy::Result<()> {
        let lst = &mut self.content.child;
        if let Some(t) = lst.selected() {
            store::get().delete_todo(t.todo.id).unwrap();
            lst.delete_selected(c);
        }
        Ok(())
    }

    /// Accept the new item we're currently editing.
    #[command]
    fn accept_add(&mut self, _: &mut dyn Core) -> canopy::Result<()> {
        if let Some(adder) = &mut self.adder {
            let item = store::get().add_todo(&adder.child.text()).unwrap();
            self.content.child.append(TodoItem::new(item));
            self.adder = None;
        }
        Ok(())
    }

    /// Close the add item editor.
    #[command]
    fn cancel_add(&mut self, _: &mut dyn Core) -> canopy::Result<()> {
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
    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> canopy::Result<()> {
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
    cnpy.load_commands::<List<TodoItem>>();
    cnpy.load_commands::<Todo>();

    canopy::MapBuilder::new()
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
        .with_path("input_line")
        .key(key::KeyCode::Enter, "todo::accept_add()")
        .key(key::KeyCode::Esc, "todo::cancel_add()")
        .build(&mut cnpy)?;

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

        runloop(
            cnpy,
            Inspector::new(key::Ctrl + key::KeyCode::Right, Todo::new()?),
        )?;
    } else {
        println!("Specify a file path")
    }

    Ok(())
}
