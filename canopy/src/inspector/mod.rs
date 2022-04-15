mod logs;
mod statusbar;
mod view;

use crate as canopy;
use crate::{
    event::key, widgets::frame, BackendControl, Node, NodeState, Outcome, Render, Result,
    StatefulNode, ViewPort,
};

#[derive(Debug, PartialEq, Clone)]
struct InspectorState {}

#[derive(StatefulNode)]

pub struct Content {
    state: NodeState,
    view: frame::Frame<view::View>,
    statusbar: statusbar::StatusBar,
}

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
    fn render(&mut self, _r: &mut Render, vp: ViewPort) -> Result<()> {
        let parts = vp.carve_vend(1);
        self.statusbar.wrap(parts.1)?;
        self.view.wrap(parts.0)?;
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
    root: N,
    active: bool,
    activate: key::Key,
    content: Content,
}

impl<N> Inspector<N>
where
    N: Node,
{
    pub fn new(activate: key::Key, root: N) -> Self {
        let mut content = Content::new();
        content.hide();
        Inspector {
            state: NodeState::default(),
            active: false,
            content,
            root,
            activate,
        }
    }

    pub fn hide(&mut self) -> Result<Outcome> {
        self.active = false;
        self.content.hide();
        canopy::taint_tree(self);
        canopy::focus_first(&mut self.root)?;
        Ok(Outcome::handle())
    }

    pub fn show(&mut self) -> Result<Outcome> {
        self.active = true;
        self.content.unhide();
        canopy::taint_tree(self);
        canopy::focus_first(self)?;
        Ok(Outcome::handle())
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
                    canopy::focus_first(&mut self.root)?;
                }
                c if c == 'q' => {
                    self.hide()?;
                }
                c if c == self.activate => {
                    if canopy::on_focus_path(&mut self.content) {
                        self.hide()?;
                    } else {
                        canopy::focus_first(self)?;
                    }
                }
                _ => return Ok(Outcome::ignore()),
            };
        } else if k == self.activate {
            self.show()?;
        };
        Ok(Outcome::handle())
    }

    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        r.style.push_layer("inspector");
        if self.active {
            let parts = vp.split_horizontal(2)?;
            self.content.wrap(parts[0])?;
            self.root.wrap(parts[1])?;
        } else {
            self.root.wrap(vp)?;
        };
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.content)?;
        f(&mut self.root)
    }
}
