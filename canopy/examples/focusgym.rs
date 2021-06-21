use anyhow::Result;
use crossterm::style::Color;
use std::io::Write;

use canopy;
use canopy::{
    event::{key, mouse},
    layout::FixedLayout,
    runloop::runloop,
    widgets::{block, solid_frame},
    Canopy, EventResult, Node, NodeState, Rect, StatefulNode,
};

struct Handle {}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: Block,
}

impl Root {
    fn new() -> Self {
        Root {
            state: NodeState::default(),
            child: Block {
                state: NodeState::default(),
                children: vec![Block::new(false), Block::new(false)],
                horizontal: true,
            },
        }
    }
}

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    children: Vec<Block>,
    horizontal: bool,
}

impl Block {
    fn new(orientation: bool) -> Self {
        Block {
            state: NodeState::default(),
            children: vec![],
            horizontal: orientation,
        }
    }
}

impl FixedLayout<Handle> for Root {
    fn layout(&mut self, app: &mut Canopy<Handle>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        if let Some(a) = rect {
            app.resize(&mut self.child, a)?;
        }
        Ok(())
    }
}

impl Node<Handle> for Root {
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => app.focus_next(self)?,
            c if c == mouse::Action::ScrollUp => app.focus_prev(self)?,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == key::KeyCode::Tab => app.focus_next(self)?,
            c if c == 'l' || c == key::KeyCode::Right => app.focus_right(self)?,
            c if c == 'h' || c == key::KeyCode::Left => app.focus_left(self)?,
            c if c == 'j' || c == key::KeyCode::Down => app.focus_down(self)?,
            c if c == 'k' || c == key::KeyCode::Up => app.focus_up(self)?,
            c if c == 'q' => EventResult::Exit,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node<Handle>) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

impl Block {
    fn add(&mut self, app: &mut Canopy<Handle>) -> Result<EventResult> {
        Ok(if self.children.len() == 0 {
            EventResult::Ignore { skip: false }
        } else if self.size_limited() {
            EventResult::Handle { skip: false }
        } else {
            self.children.push(Block::new(!self.horizontal));
            self.layout(app, self.rect())?;
            app.taint_tree(self)?;
            EventResult::Handle { skip: false }
        })
    }
    fn size_limited(&self) -> bool {
        if let Some(a) = self.rect() {
            if self.horizontal && a.w <= 4 {
                true
            } else if !self.horizontal && a.h <= 4 {
                true
            } else {
                false
            }
        } else {
            false
        }
    }
    fn split(&mut self, app: &mut Canopy<Handle>) -> Result<EventResult> {
        Ok(if self.children.len() != 0 {
            EventResult::Ignore { skip: false }
        } else if self.size_limited() {
            EventResult::Handle { skip: false }
        } else {
            self.children = vec![Block::new(!self.horizontal), Block::new(!self.horizontal)];
            self.layout(app, self.rect())?;
            app.taint_tree(self)?;
            EventResult::Handle { skip: false }
        })
    }
}

impl FixedLayout<Handle> for Block {
    fn layout(&mut self, app: &mut Canopy<Handle>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        if let Some(a) = rect {
            if self.children.len() > 0 {
                let sizes = if self.horizontal {
                    a.split_horizontal(self.children.len() as u16)?
                } else {
                    a.split_vertical(self.children.len() as u16)?
                };
                for i in 0..self.children.len() {
                    app.resize(&mut self.children[i], sizes[i])?
                }
            }
        }
        Ok(())
    }
}

impl Node<Handle> for Block {
    fn can_focus(&self) -> bool {
        self.children.len() == 0
    }
    fn render(&mut self, app: &mut Canopy<Handle>, w: &mut dyn Write) -> Result<()> {
        if let Some(a) = self.rect() {
            if self.children.len() == 0 {
                block(
                    w,
                    a.inner(1)?,
                    if app.is_focused(self) && self.children.len() == 0 {
                        Color::Magenta
                    } else {
                        Color::Blue
                    },
                    '\u{2588}',
                )?;
                solid_frame(w, a.frame(1)?, Color::Black, ' ')?;
            }
        }
        Ok(())
    }
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == mouse::Action::Down + mouse::Button::Left => {
                app.taint_tree(self)?;
                app.set_focus(self)?
            }
            c if c == mouse::Action::Down + mouse::Button::Middle => {
                self.split(app)?;
                if app.is_focused(self) {
                    app.focus_next(self)?;
                };
                EventResult::Handle { skip: false }
            }
            c if c == mouse::Action::Down + mouse::Button::Right => self.add(app)?,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == 's' => {
                self.split(app)?;
                app.focus_next(self)?
            }
            c if c == 'a' => self.add(app)?,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node<Handle>) -> Result<()>) -> Result<()> {
        for i in &mut self.children {
            f(i)?
        }
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut h = Handle {};
    let mut app = Canopy::new();
    let mut root = Root::new();
    runloop(&mut app, &mut root, &mut h)
}
