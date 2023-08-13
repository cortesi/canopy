use crate as canopy;
use crate::{
    event::key,
    geom::Expanse,
    state::{NodeState, StatefulNode},
    *,
};
use tracing;

use super::core;
use super::*;

#[derive(StatefulNode)]
pub struct EditorView {
    state: NodeState,
    core: core::Core,
    /// Line offset of the window into the text buffer.
    window_offset: usize,
}

#[derive_commands]
impl EditorView {
    pub fn new(txt: &str) -> Self {
        EditorView {
            state: NodeState::default(),
            core: core::Core::new(txt),
            window_offset: 0,
        }
    }
}

impl Node for EditorView {
    fn cursor(&self) -> Option<cursor::Cursor> {
        let p = self.core.cursor_position(Window::from_offset(
            &self.core.state,
            self.window_offset,
            self.vp().screen_rect().h as usize,
        ));
        if let Some(p) = p {
            Some(cursor::Cursor {
                location: p,
                shape: cursor::CursorShape::Block,
                blink: true,
            })
        } else {
            None
        }
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _: &dyn Core, r: &mut Render) -> Result<()> {
        let vo = self.vp().view_rect();
        for (i, s) in self
            .core
            .state
            .wrapped_text(Window::from_offset(
                &self.core.state,
                self.window_offset,
                vo.h as usize,
            ))
            .iter()
            .enumerate()
        {
            if let Some(t) = s {
                r.text("text", vo.line(i as u16), t)?;
            }
        }
        Ok(())
    }

    fn fit(&mut self, sz: Expanse) -> Result<Expanse> {
        self.core.set_width(sz.w as usize);
        Ok(Expanse::new(sz.w, self.core.state.wrapped_height() as u16))
    }
}

/// A simple editor
#[derive(StatefulNode)]
pub struct Editor {
    state: NodeState,
    view: EditorView,
}

#[derive_commands]
impl Editor {
    pub fn new(txt: &str) -> Self {
        Editor {
            state: NodeState::default(),
            view: EditorView::new(txt),
        }
    }

    /// Move the cursor left.
    #[command]
    fn cursor_left(&mut self, _: &dyn Core) {
        tracing::info!("cursor_left")
    }

    /// Move the cursor right.
    #[command]
    fn cursor_right(&mut self, _: &dyn Core) {
        tracing::info!("cursor_right")
    }
}

impl DefaultBindings for Editor {
    fn defaults(b: Binder) -> Binder {
        b.key(key::KeyCode::Left, "editor::cursor_left()")
            .key(key::KeyCode::Right, "editor::cursor_right()")
    }
}

impl Node for Editor {
    fn accept_focus(&mut self) -> bool {
        true
    }

    fn fit(&mut self, sz: Expanse) -> Result<Expanse> {
        self.view.fit(sz)
    }

    fn render(&mut self, _c: &dyn Core, _: &mut Render) -> Result<()> {
        let vp = self.vp();
        fit(&mut self.view, vp)?;
        self.set_viewport(self.view.vp());
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}
