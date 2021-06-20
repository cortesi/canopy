use anyhow::Result;
use crossterm::style::Color;

use canopy;
use canopy::{
    event::{key, mouse},
    geom::{Point, Rect},
    layout::FixedLayout,
    runloop::runloop,
    state::{NodeState, StatefulNode},
    widgets::{frame, input, scroll, text},
    Canopy, EventResult, Node,
};

struct Handle {}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    child: frame::Frame<Handle, scroll::Scroll<Handle, text::Text<Handle>>>,
    adder: Option<frame::Frame<Handle, input::Input<Handle>>>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            child: frame::Frame::new(
                scroll::Scroll::new(text::Text::new(&contents)),
                frame::SINGLE,
                Color::White,
                Color::Blue,
            ),
            adder: None,
        }
    }
    fn open_adder(&mut self, app: &mut Canopy<Handle>) -> Result<EventResult> {
        let mut adder = frame::Frame::new(
            input::Input::new(15),
            frame::SINGLE,
            Color::White,
            Color::Blue,
        );
        app.set_focus(&mut adder.child)?;
        self.adder = Some(adder);
        self.layout(app, self.rect())?;
        Ok(EventResult::Handle { skip: false })
    }
}

impl FixedLayout<Handle> for Root {
    fn layout(&mut self, app: &mut Canopy<Handle>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        if let Some(a) = rect {
            app.resize(&mut self.child, a)?;
            if let Some(add) = &mut self.adder {
                add.layout(
                    app,
                    Some(Rect {
                        tl: Point {
                            x: a.tl.x + 2,
                            y: a.tl.y + a.h / 2,
                        },
                        w: a.w - 4,
                        h: 3,
                    }),
                )?;
            }
        }
        Ok(())
    }
}

impl Node<Handle> for Root {
    fn can_focus(&self) -> bool {
        true
    }
    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => self.child.child.down(app)?,
            c if c == mouse::Action::ScrollUp => self.child.child.up(app)?,
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
            c if c == 'a' => self.open_adder(app)?,
            c if c == 'g' => self.child.child.scroll_to(app, 0, 0)?,
            c if c == 'j' || c == key::KeyCode::Down => self.child.child.down(app)?,
            c if c == 'k' || c == key::KeyCode::Up => self.child.child.up(app)?,
            c if c == 'h' || c == key::KeyCode::Left => self.child.child.left(app)?,
            c if c == 'l' || c == key::KeyCode::Up => self.child.child.right(app)?,
            c if c == ' ' || c == key::KeyCode::PageDown => self.child.child.page_down(app)?,
            c if c == key::KeyCode::PageUp => self.child.child.page_up(app)?,
            c if c == key::KeyCode::Enter => {
                self.adder = None;
                app.set_focus(self)?;
                app.taint_tree(self)?;
                EventResult::Handle { skip: false }
            }
            c if c == key::KeyCode::Esc => {
                self.adder = None;
                app.set_focus(self)?;
                app.taint_tree(self)?;
                EventResult::Handle { skip: false }
            }
            c if c == 'q' => EventResult::Exit,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node<Handle>) -> Result<()>) -> Result<()> {
        f(&mut self.child)?;
        if let Some(a) = &mut self.adder {
            f(a)?;
        }
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut app = Canopy::new();
    let mut h = Handle {};
    let mut root = Root::new(String::new());
    app.focus_next(&mut root)?;
    runloop(&mut app, &mut root, &mut h)?;
    Ok(())
}
