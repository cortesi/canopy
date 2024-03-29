use crate as canopy;
use crate::{
    event::key,
    geom::Expanse,
    state::{NodeState, StatefulNode},
    *,
};

use super::core;

#[derive(StatefulNode)]
pub struct EditorView {
    state: NodeState,
    core: core::Core,
}

#[derive_commands]
impl EditorView {
    pub fn new(txt: &str) -> Self {
        EditorView {
            state: NodeState::default(),
            core: core::Core::new(txt),
        }
    }
}

impl Node for EditorView {
    fn cursor(&self) -> Option<cursor::Cursor> {
        let p = self.core.cursor_position();
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
        let vo = self.vp().view;
        let sr = self.vp().screen_rect();
        self.core.resize_window(sr.w as usize, sr.h as usize);
        for (i, s) in self.core.window_text().iter().enumerate() {
            if let Some(t) = s {
                r.text("text", vo.line(i as u16), t)?;
            }
        }
        Ok(())
    }

    fn fit(&mut self, sz: Expanse) -> Result<()> {
        let outer = Expanse::new(sz.w, self.core.wrapped_height() as u16);
        self.vp_mut().fit_size(outer, sz);
        Ok(())
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

    /// Move the cursor left or right.
    #[command]
    fn cursor_shift(&mut self, _: &dyn Core, n: isize) {
        self.view.core.cursor_shift(n);
    }

    /// Move the cursor up or down in the chunk list.
    #[command]
    fn cursor_shift_chunk(&mut self, _: &dyn Core, n: isize) {
        self.view.core.cursor_shift_chunk(n);
    }

    /// Move the cursor up or down by visual line.
    #[command]
    fn cursor_shift_lines(&mut self, _: &dyn Core, n: isize) {
        self.view.core.cursor_shift_lines(n);
    }
}

impl DefaultBindings for Editor {
    fn defaults(b: Binder) -> Binder {
        b.key(key::KeyCode::Left, "editor::cursor_shift(1)")
            .key(key::KeyCode::Right, "editor::cursor_shift(-1)")
            .key(key::KeyCode::Down, "editor::cursor_shift_lines(1)")
            .key(key::KeyCode::Up, "editor::cursor_shift_lines(-1)")
            .key('h', "editor::cursor_shift(-1)")
            .key('l', "editor::cursor_shift(1)")
            .key('j', "editor::cursor_shift_chunk(1)")
            .key('k', "editor::cursor_shift_chunk(-1)")
    }
}

impl Node for Editor {
    fn accept_focus(&mut self) -> bool {
        false
    }

    fn fit(&mut self, sz: Expanse) -> Result<()> {
        fit_wrap!(self, self.view, sz);
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}
