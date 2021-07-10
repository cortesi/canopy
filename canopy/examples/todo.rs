use duplicate::duplicate;
use std::io::Write;

use canopy;
use canopy::{
    colorscheme::{solarized, ColorScheme},
    event::{key, mouse},
    geom::{Point, Rect},
    layout::FixedLayout,
    runloop::runloop,
    widgets::{block, frame, input, scroll, text},
    Canopy, EventOutcome, Node, NodeState, Result, StatefulNode,
};
use crossterm::{cursor::MoveTo, style::Print, QueueableCommand};

struct Handle {}

#[derive(StatefulNode)]
struct StatusBar {
    state: NodeState,
}

impl Node<Handle> for StatusBar {
    /// Render the widget to a buffer. The default implementation does nothing.
    fn render(
        &self,
        _app: &Canopy<Handle>,
        colors: &mut ColorScheme,
        w: &mut dyn Write,
    ) -> Result<()> {
        colors.push_layer("statusbar");
        colors.set("statusbar/text", w)?;
        if let Some(r) = self.rect() {
            block(w, r, ' ')?;
            w.queue(MoveTo(r.tl.x, r.tl.y))?;
            w.queue(Print("todo"))?;
        }
        Ok(())
    }
}

impl FixedLayout<Handle> for StatusBar {
    fn layout(&mut self, _app: &mut Canopy<Handle>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<Handle, scroll::Scroll<Handle, text::Text<Handle>>>,
    statusbar: StatusBar,
    adder: Option<frame::Frame<Handle, input::InputLine<Handle>>>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(scroll::Scroll::new(text::Text::new(&contents))),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
            adder: None,
        }
    }
    fn open_adder(&mut self, app: &mut Canopy<Handle>) -> Result<EventOutcome> {
        let mut adder = frame::Frame::new(input::InputLine::new(""));
        app.set_focus(&mut adder.child)?;
        self.adder = Some(adder);
        self.layout(app, self.rect())?;
        Ok(EventOutcome::Handle { skip: false })
    }
}

impl FixedLayout<Handle> for Root {
    fn layout(&mut self, app: &mut Canopy<Handle>, rect: Option<Rect>) -> Result<()> {
        self.set_rect(rect);
        if let Some(a) = rect {
            if a.h > 2 {
                let sb = Rect {
                    tl: Point {
                        x: a.tl.x,
                        y: a.tl.y + a.h - 1,
                    },
                    w: a.w,
                    h: 1,
                };
                let ct = Rect {
                    tl: a.tl,
                    w: a.w,
                    h: a.h - 1,
                };
                app.resize(&mut self.statusbar, sb)?;
                app.resize(&mut self.content, ct)?;
            } else {
                app.resize(&mut self.content, a)?;
            }
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
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => self.content.child.down(app)?,
            c if c == mouse::Action::ScrollUp => self.content.child.up(app)?,
            _ => EventOutcome::Ignore { skip: false },
        })
    }
    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<EventOutcome> {
        Ok(match k {
            c if c == 'a' => self.open_adder(app)?,
            c if c == 'g' => self.content.child.scroll_to(app, 0, 0)?,
            c if c == 'j' || c == key::KeyCode::Down => self.content.child.down(app)?,
            c if c == 'k' || c == key::KeyCode::Up => self.content.child.up(app)?,
            c if c == 'h' || c == key::KeyCode::Left => self.content.child.left(app)?,
            c if c == 'l' || c == key::KeyCode::Up => self.content.child.right(app)?,
            c if c == ' ' || c == key::KeyCode::PageDown => self.content.child.page_down(app)?,
            c if c == key::KeyCode::PageUp => self.content.child.page_up(app)?,
            c if c == key::KeyCode::Enter => {
                self.adder = None;
                app.taint_tree(self)?;
                EventOutcome::Handle { skip: false }
            }
            c if c == key::KeyCode::Esc => {
                self.adder = None;
                app.taint_tree(self)?;
                EventOutcome::Handle { skip: false }
            }
            c if c == 'q' => EventOutcome::Exit,
            _ => EventOutcome::Ignore { skip: false },
        })
    }

    #[duplicate(
        method          reference(type);
        [children]      [& type];
        [children_mut]  [&mut type];
    )]
    fn method(
        self: reference([Self]),
        f: &mut dyn FnMut(reference([dyn Node<Handle>])) -> Result<()>,
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
    let mut app = Canopy::new();
    let mut h = Handle {};
    let mut root = Root::new(String::new());
    let mut colors = solarized::solarized_dark();
    colors.insert("statusbar", Some(solarized::BASE02), Some(solarized::BASE1));
    runloop(&mut app, &mut colors, &mut root, &mut h)?;
    Ok(())
}
