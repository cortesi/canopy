mod logs;
mod view;

use canopy_core as canopy;

use crate::{frame, tabs};
use canopy_core::{
    Binder, Canopy, DefaultBindings, Loader, NodeState, derive_commands, event::key::*, *,
};

use logs::Logs;

#[derive(canopy_core::StatefulNode)]

pub struct Inspector {
    state: NodeState,
    view: frame::Frame<view::View>,
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Inspector {
    pub fn new() -> Self {
        let mut i = Inspector {
            state: NodeState::default(),
            view: frame::Frame::new(view::View::new()),
        };
        i.hide();
        i
    }
}

impl Node for Inspector {
    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.place(&mut self.view, sz.into())?;
        Ok(())
    }

    fn render(&mut self, _c: &dyn Context, r: &mut Render) -> Result<()> {
        r.style.push_layer("inspector");
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}

impl DefaultBindings for Inspector {
    fn defaults(b: Binder) -> Binder {
        b.with_path("inspector/")
            .key(KeyCode::Tab, "tabs::next()")
            .with_path("logs")
            .key('C', "list::clear()")
            .key('d', "list::delete_selected()")
            .key('j', "list::select_next()")
            .key('k', "list::select_prev()")
            .key('g', "list::select_first()")
            .key('G', "list::select_last()")
            .key(' ', "list::page_down()")
            .key(KeyCode::PageDown, "list::page_down()")
            .key(KeyCode::PageUp, "list::page_up()")
            .key(KeyCode::Down, "list::select_next()")
            .key(KeyCode::Down, "list::select_prev()")
    }
}

impl Loader for Inspector {
    fn load(c: &mut Canopy) {
        c.add_commands::<Inspector>();
        c.add_commands::<tabs::Tabs>();
        Logs::load(c);
    }
}
