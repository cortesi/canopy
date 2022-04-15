use crate as canopy;
use crate::{
    geom::{Expanse, Rect},
    widgets::{list::*, Text},
    Node, NodeState, Render, Result, StatefulNode, ViewPort,
};
use std::time::Duration;

#[derive(StatefulNode)]
struct LogItem {
    state: NodeState,
    selected: bool,
    child: Text,
}

impl LogItem {
    fn new(txt: &str) -> Self {
        LogItem {
            state: NodeState::default(),
            selected: false,
            child: Text::new(txt),
        }
    }
}

impl ListItem for LogItem {
    fn set_selected(&mut self, state: bool) {
        self.selected = state
    }
}

impl Node for LogItem {
    fn fit(&mut self, target: Expanse) -> Result<Expanse> {
        self.child.fit(Expanse {
            w: target.w - 2,
            h: target.h,
        })
    }

    fn render(&mut self, r: &mut Render, vp: ViewPort) -> Result<()> {
        let (_, screen) = vp.screen_rect().carve_hstart(2);
        let outer = self.child.fit(screen.into())?;
        let view = Rect {
            tl: vp.view_rect().tl,
            w: vp.view_rect().w.saturating_sub(2),
            h: vp.view_rect().h,
        };
        self.child
            .set_viewport(ViewPort::new(outer, view, screen.tl)?);

        let v = vp.view_rect();
        let status = Rect::new(v.tl.x, v.tl.y, 1, v.h);
        if self.selected {
            r.fill("blue", status, '\u{2588}')?;
        } else {
            r.fill("", status, ' ')?;
        }
        let buf = Rect::new(v.tl.x + 1, v.tl.y, 1, v.h);
        r.fill("", buf, ' ')?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.child)
    }
}

#[derive(StatefulNode)]
pub struct Logs {
    state: NodeState,
    list: List<LogItem>,
}

impl Node for Logs {
    fn poll(&mut self) -> Option<Duration> {
        self.list.append(LogItem::new("fooob"));
        Some(Duration::from_millis(1000))
    }

    fn render(&mut self, _: &mut Render, vp: ViewPort) -> Result<()> {
        self.list.wrap(vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.list)?;
        Ok(())
    }
}

impl Logs {
    pub fn new() -> Self {
        Logs {
            state: NodeState::default(),
            list: List::new(vec![]),
        }
    }
}
