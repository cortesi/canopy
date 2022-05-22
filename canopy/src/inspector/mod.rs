mod logs;
mod statusbar;
mod view;

use crate as canopy;
use crate::{
    derive_commands, event::key, fit, focus, widgets::frame, BackendControl, Node, NodeState,
    Outcome, Render, Result, StatefulNode,
};

#[derive(StatefulNode)]

pub struct Content {
    state: NodeState,
    view: frame::Frame<view::View>,
    statusbar: statusbar::StatusBar,
}

#[derive_commands]
impl Content {
    pub fn new() -> Self {
        Content {
            state: NodeState::default(),
            view: frame::Frame::new(view::View::new()),
            statusbar: statusbar::StatusBar::new(),
        }
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::new()
    }
}

impl Node for Content {
    fn render(&mut self, r: &mut Render) -> Result<()> {
        r.style.push_layer("inspector");
        let parts = self.vp().carve_vend(1);
        fit(&mut self.statusbar, parts.1)?;
        fit(&mut self.view, parts.0)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.statusbar)?;
        f(&mut self.view)
    }
}

#[derive(StatefulNode)]
pub struct Inspector<N>
where
    N: Node,
{
    state: NodeState,
    app: N,
    active: bool,
    activate: key::Key,
    content: Content,
}

#[derive_commands]
impl<N> Inspector<N>
where
    N: Node,
{
    pub fn new(activate: key::Key, root: N) -> Self {
        let mut c = Inspector {
            state: NodeState::default(),
            active: false,
            content: Content::new(),
            app: root,
            activate,
        };
        c.hide().unwrap();
        c
    }

    pub fn hide(&mut self) -> Result<Outcome> {
        self.active = false;
        self.content.hide();
        canopy::taint_tree(self);
        focus::shift_first(&mut self.app)?;
        Ok(Outcome::Handle)
    }

    pub fn show(&mut self) -> Result<Outcome> {
        self.active = true;
        self.content.unhide();
        canopy::taint_tree(self);
        focus::shift_first(&mut self.content)?;
        Ok(Outcome::Handle)
    }
}

impl<N> Node for Inspector<N>
where
    N: Node,
{
    fn handle_key(&mut self, _ctrl: &mut dyn BackendControl, k: key::Key) -> Result<Outcome> {
        if self.active {
            match k {
                c if c == 'a' => {
                    focus::shift_first(&mut self.app)?;
                }
                c if c == 'q' => {
                    self.hide()?;
                }
                c if c == self.activate => {
                    if focus::is_on_path(&mut self.content) {
                        self.hide()?;
                    } else {
                        focus::shift_first(&mut self.content)?;
                    }
                }
                _ => return Ok(Outcome::Ignore),
            };
        } else if k == self.activate {
            self.show()?;
        };
        Ok(Outcome::Handle)
    }

    fn render(&mut self, _r: &mut Render) -> Result<()> {
        let vp = self.vp();
        if self.active {
            let parts = vp.split_horizontal(2)?;
            fit(&mut self.content, parts[0])?;
            fit(&mut self.app, parts[1])?;
        } else {
            fit(&mut self.app, vp)?;
        };
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.app)?;
        f(&mut self.content)
    }
}
