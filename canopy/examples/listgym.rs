use rand::seq::SliceRandom;
use rand::Rng;

use canopy;
use canopy::{
    event::{key, mouse},
    geom::{Rect, Size},
    render::term::runloop,
    style::solarized,
    widgets::{frame, list::*, Text},
    Canopy, Node, NodeState, Outcome, Result, StatefulNode, ViewPort,
};

const TEXT: &str = "What a struggle must have gone on during long centuries between the several kinds of trees, each annually scattering its seeds by the thousand; what war between insect and insect — between insects, snails, and other animals with birds and beasts of prey — all striving to increase, all feeding on each other, or on the trees, their seeds and seedlings, or on the other plants which first clothed the ground and thus checked the growth of the trees.";

const COLORS: &'static [&str] = &["red", "blue", "green"];

struct Handle {}

#[derive(StatefulNode)]
struct Block {
    state: NodeState,
    child: Text<Handle>,
    color: String,
    selected: bool,
}

impl Block {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Block {
            state: NodeState::default(),
            child: Text::new(TEXT).with_fixed_width(rng.gen_range(10..150)),
            color: String::from(*(COLORS.choose(&mut rng).unwrap())),
            selected: false,
        }
    }
}

impl ListItem for Block {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node<Handle, ()> for Block {
    fn fit(&mut self, app: &mut Canopy<Handle, ()>, target: Size) -> Result<Size> {
        self.child.fit(
            app,
            Size {
                w: target.w - 2,
                h: target.h,
            },
        )
    }

    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        let [_, screen] = vp.screen_rect().carve_hstart(2);
        let outer = self.child.fit(app, screen.into())?;
        let view = Rect {
            tl: vp.view_rect().tl,
            w: vp.view_rect().w.saturating_sub(2),
            h: vp.view_rect().h,
        };
        self.child.state_mut().viewport = ViewPort::new(outer, view, screen.tl)?;

        let v = vp.view_rect();
        let status = Rect::new(v.tl.x, v.tl.y, 1, v.h);
        if self.selected {
            app.render.fill("blue", status, '\u{2588}')?;
        } else {
            app.render.fill("", status, ' ')?;
        }
        let buf = Rect::new(v.tl.x + 1, v.tl.y, 1, v.h);
        app.render.fill("", buf, ' ')?;
        app.render.style.push_layer(&self.color);

        Ok(())
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
}

#[derive(StatefulNode)]
struct StatusBar {
    state: NodeState,
}

impl Node<Handle, ()> for StatusBar {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        app.render.style.push_layer("statusbar");
        app.render
            .text("text", vp.view_rect().first_line(), "listgym")?;
        Ok(())
    }
}

#[derive(StatefulNode)]
struct Root {
    state: NodeState,
    content: frame::Frame<Handle, (), List<Handle, (), Block>>,
    statusbar: StatusBar,
}

impl Root {
    fn new() -> Self {
        let nodes: Vec<Block> = (0..10).map(|_| Block::new()).collect();
        Root {
            state: NodeState::default(),
            content: frame::Frame::new(List::new(nodes)),
            statusbar: StatusBar {
                state: NodeState::default(),
            },
        }
    }
}

impl Node<Handle, ()> for Root {
    fn render(&mut self, app: &mut Canopy<Handle, ()>, vp: ViewPort) -> Result<()> {
        let parts = vp.carve_vend(1)?;
        self.statusbar.wrap(app, parts[1])?;
        self.content.wrap(app, parts[0])?;
        Ok(())
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn handle_mouse(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: mouse::Mouse,
    ) -> Result<Outcome<()>> {
        let v = &mut self.content.child.state_mut().viewport;
        match k {
            c if c == mouse::Action::ScrollDown => v.down(),
            c if c == mouse::Action::ScrollUp => v.up(),
            _ => return Ok(Outcome::ignore()),
        };
        app.taint_tree(self)?;
        Ok(Outcome::handle())
    }

    fn handle_key(
        &mut self,
        app: &mut Canopy<Handle, ()>,
        _: &mut Handle,
        k: key::Key,
    ) -> Result<Outcome<()>> {
        let lst = &mut self.content.child;
        match k {
            c if c == 'a' => {
                lst.insert_after(Block::new());
            }
            c if c == 'A' => {
                lst.append(Block::new());
            }
            c if c == 'C' => {
                lst.clear();
            }
            c if c == 'd' => {
                lst.delete_selected();
            }
            c if c == 'g' => lst.select_first(),
            c if c == 'G' => lst.select_last(),
            c if c == 'J' => lst.scroll_down(),
            c if c == 'K' => lst.scroll_up(),
            c if c == 'j' || c == key::KeyCode::Down => lst.select_next(),
            c if c == 'k' || c == key::KeyCode::Up => lst.select_prev(),
            c if c == 'h' || c == key::KeyCode::Left => lst.scroll_left(),
            c if c == 'l' || c == key::KeyCode::Right => lst.scroll_right(),
            c if c == ' ' || c == key::KeyCode::PageDown => lst.page_down(),
            c if c == key::KeyCode::PageUp => lst.page_up(),
            c if c == 'q' => app.exit(0),
            _ => return Ok(Outcome::ignore()),
        };
        app.taint_tree(self)?;
        Ok(Outcome::handle())
    }

    fn children(&self, f: &mut dyn FnMut(&dyn Node<Handle, ()>) -> Result<()>) -> Result<()> {
        f(&self.statusbar)?;
        f(&self.content)?;
        Ok(())
    }

    fn children_mut(
        &mut self,
        f: &mut dyn FnMut(&mut dyn Node<Handle, ()>) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.content)?;
        Ok(())
    }
}

pub fn main() -> Result<()> {
    let mut colors = solarized::solarized_dark();
    colors.add(
        "red/text",
        Some(solarized::RED),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    colors.add(
        "blue/text",
        Some(solarized::BLUE),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    colors.add(
        "green/text",
        Some(solarized::GREEN),
        None,
        Some(canopy::style::AttrSet::default()),
    );
    colors.add(
        "statusbar/text",
        Some(solarized::BLUE),
        None,
        Some(canopy::style::AttrSet::default()),
    );

    let mut h = Handle {};
    let mut root = Root::new();
    runloop(colors, &mut root, &mut h)?;
    Ok(())
}
