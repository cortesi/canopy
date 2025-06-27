use anyhow::Result;
use canopy::{
    event::{key, mouse},
    geom::{Expanse, Rect},
    style::solarized,
    widgets::{frame, list::*, Input, Text},
    *,
};

pub mod store;

#[derive(StatefulNode)]
pub struct TodoItem {
    pub state: NodeState,
    pub child: Text,
    pub selected: bool,
    pub todo: store::Todo,
}

impl TodoItem {
    pub fn new(t: store::Todo) -> Self {
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
        l.wrap(self, vp)?;
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
pub struct StatusBar {
    pub state: NodeState,
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
pub struct Todo {
    pub state: NodeState,
    pub content: frame::Frame<List<TodoItem>>,
    pub statusbar: StatusBar,
    pub adder: Option<frame::Frame<Input>>,
}

#[derive_commands]
impl Todo {
    pub fn new() -> Result<Self> {
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

    #[command]
    pub fn enter_item(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        let mut adder = frame::Frame::new(Input::new(""));
        c.set_focus(&mut adder.child);
        self.adder = Some(adder);
        c.taint(self);
        Ok(())
    }

    #[command]
    pub fn delete_item(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        let lst = &mut self.content.child;
        if let Some(t) = lst.selected() {
            store::get().delete_todo(t.todo.id).unwrap();
            lst.delete_selected(c);
            c.taint_tree(self);
        }
        Ok(())
    }

    #[command]
    pub fn accept_add(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        if let Some(adder) = self.adder.take() {
            let value = adder.child.textbuf.value;
            if !value.is_empty() {
                let item = store::get().add_todo(&value).unwrap();
                self.content.child.append(TodoItem::new(item));
                // Select the newly added item (which is the last one)
                let new_index = self.content.child.len().saturating_sub(1);
                self.content.child.select(new_index);
            }
        }
        c.taint_tree(self);
        c.set_focus(&mut self.content);
        Ok(())
    }

    #[command]
    pub fn cancel_add(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.adder = None;
        c.taint_tree(self);
        c.focus_first(self);
        Ok(())
    }

    fn load(&mut self) -> canopy::Result<()> {
        let s = store::get().todos().unwrap();
        let todos = s.iter().map(|x| TodoItem::new(x.clone()));
        for i in todos {
            self.content.child.append(i);
        }
        // Select the first item if any exist
        if !self.content.child.is_empty() {
            self.content.child.select(0);
        }
        Ok(())
    }

    #[command]
    pub fn select_first(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.content.child.select_first(c);
        Ok(())
    }

    #[command]
    pub fn select_next(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.content.child.select_next(c);
        Ok(())
    }

    #[command]
    pub fn select_prev(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.content.child.select_prev(c);
        Ok(())
    }

    #[command]
    pub fn page_down(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.content.child.page_down(c);
        Ok(())
    }

    #[command]
    pub fn page_up(&mut self, c: &mut dyn Context) -> canopy::Result<()> {
        self.content.child.page_up(c);
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
            l.place(add, vp, Rect::new(a.tl.x + 2, a.tl.y + a.h / 2, a.w - 4, 3))?;
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

impl Loader for Todo {
    fn load(c: &mut Canopy) {
        c.add_commands::<Todo>();
        c.add_commands::<List<TodoItem>>();
        c.add_commands::<Input>();
    }
}

pub fn style(cnpy: &mut Canopy) {
    cnpy.style.add(
        "statusbar/text",
        Some(solarized::BASE02),
        Some(solarized::BASE1),
        None,
    );
    // Ensure text under blue layer gets blue foreground
    cnpy.style.add_fg("blue/text", solarized::BLUE);
}

pub fn bind_keys(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .with_path("todo/")
        .key('q', "root::quit()")
        .key('d', "todo::delete_item()")
        .key('a', "todo::enter_item()")
        .key('g', "todo::select_first()")
        .key('j', "todo::select_next()")
        .key(key::KeyCode::Down, "todo::select_next()")
        .key('k', "todo::select_prev()")
        .key(key::KeyCode::Up, "todo::select_prev()")
        .key(' ', "todo::page_down()")
        .key(key::KeyCode::PageDown, "todo::page_down()")
        .key(key::KeyCode::PageUp, "todo::page_up()")
        .mouse(mouse::Action::ScrollUp, "todo::select_prev()")
        .mouse(mouse::Action::ScrollDown, "todo::select_next()")
        .with_path("input")
        .defaults::<Input>()
        .key(key::KeyCode::Enter, "todo::accept_add()")
        .key(key::KeyCode::Esc, "todo::cancel_add()");
}

pub fn open_store(path: &str) -> Result<()> {
    store::open(path)
}

pub fn setup_app(cnpy: &mut Canopy) {
    <Todo as Loader>::load(cnpy);
    style(cnpy);
    bind_keys(cnpy);
}

pub fn create_app(db_path: &str) -> Result<(Canopy, Todo)> {
    open_store(db_path)?;

    let mut cnpy = Canopy::new();
    setup_app(&mut cnpy);

    let todo = Todo::new()?;
    Ok((cnpy, todo))
}
