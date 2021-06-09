use std::env;
use std::fs;
use std::io::Write;

use anyhow::Result;
use crossterm::style::Color;

use canopy;
use canopy::{
    app::Canopy,
    event::{key, mouse},
    geom::Rect,
    layout::FixedLayout,
    runloop::runloop,
    widgets::{frame, scroll, text},
    EventResult, Node, NodeState,
};

struct Handle {}

struct Root {
    state: NodeState,
    child: frame::Frame<Handle, scroll::Scroll<Handle, text::Text<Handle>>>,
    rect: Option<Rect>,
}

impl Root {
    fn new(contents: String) -> Self {
        Root {
            state: NodeState::default(),
            rect: None,
            child: frame::Frame::new(
                scroll::Scroll::new(text::Text::new(&contents)),
                frame::SINGLE,
                Color::White,
                Color::Blue,
            ),
        }
    }
}

impl FixedLayout for Root {
    fn layout(&mut self, app: &mut Canopy, rect: Option<Rect>) -> Result<()> {
        self.rect = rect;
        if let Some(a) = rect {
            app.resize(&mut self.child, a)?;
        }
        Ok(())
    }
}

impl Node<Handle> for Root {
    fn can_focus(&self) -> bool {
        true
    }
    fn state(&mut self) -> &mut NodeState {
        &mut self.state
    }
    fn rect(&self) -> Option<Rect> {
        self.rect
    }
    fn handle_mouse(
        &mut self,
        app: &mut Canopy,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<EventResult> {
        Ok(match k {
            c if c == mouse::Action::ScrollDown => app.focus_next(self)?,
            c if c == mouse::Action::ScrollUp => app.focus_prev(self)?,
            _ => EventResult::Ignore { skip: false },
        })
    }
    fn handle_key(&mut self, app: &mut Canopy, _: &mut Handle, k: key::Key) -> Result<EventResult> {
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

pub fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut app = Canopy::new();
        let mut h = Handle {};
        let contents = fs::read_to_string(args[1].clone())?;
        let mut root = Root::new(contents);
        app.focus_next(&mut root)?;
        runloop(&mut app, &mut root, &mut h)?;
    }

    Ok(())
}
