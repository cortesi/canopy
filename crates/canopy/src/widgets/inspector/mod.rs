/// Log panel widget.
mod logs;
/// Inspector view layout.
mod view;

use crate as canopy;
use crate::{
    Binder, Canopy, DefaultBindings, Loader, NodeState, derive_commands, event::key::*, *,
};
use logs::Logs;

use crate::widgets::{frame, tabs};

/// Inspector overlay node.
#[derive(StatefulNode)]
pub struct Inspector {
    /// Node state.
    state: NodeState,
    /// Root view frame.
    view: frame::Frame<view::View>,
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Inspector {
    /// Construct a new inspector.
    pub fn new() -> Self {
        let mut i = Self {
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
        c.add_commands::<Self>();
        c.add_commands::<tabs::Tabs>();
        Logs::load(c);
    }
}
