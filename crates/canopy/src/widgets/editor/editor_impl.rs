use crate::{
    Context, Layout, Node, NodeState, Render, Result, StatefulNode, command, cursor,
    derive_commands, geom::Expanse,
};

use super::core;

#[derive(StatefulNode)]
/// Internal editor view node.
pub struct EditorView {
    /// Node state.
    state: NodeState,
    /// Core editor state and logic.
    core: core::Core,
}

#[derive_commands]
impl EditorView {
    /// Construct a new editor view.
    pub fn new(txt: &str) -> Self {
        Self {
            state: NodeState::default(),
            core: core::Core::new(txt),
        }
    }
}

impl Node for EditorView {
    fn cursor(&self) -> Option<cursor::Cursor> {
        let p = self.core.cursor_position();
        p.map(|p| cursor::Cursor {
            location: p,
            shape: cursor::CursorShape::Block,
            blink: true,
        })
    }

    fn accept_focus(&mut self) -> bool {
        true
    }

    fn render(&mut self, _: &dyn Context, r: &mut Render) -> Result<()> {
        let vo = self.vp().view();
        let sr = self.vp().screen_rect();
        self.core.resize_window(sr.w as usize, sr.h as usize);
        for (i, s) in self.core.window_text().iter().enumerate() {
            if let Some(t) = s {
                r.text("text", vo.line(i as u32), t)?;
            }
        }
        Ok(())
    }

    fn layout(&mut self, _l: &Layout, sz: Expanse) -> Result<()> {
        let outer = Expanse::new(sz.w, self.core.wrapped_height() as u32);
        self.fit_size(outer, sz);
        Ok(())
    }
}

/// A simple editor
#[derive(StatefulNode)]
pub struct Editor {
    /// Node state.
    state: NodeState,
    /// Editor view node.
    view: EditorView,
}

#[derive_commands]
impl Editor {
    /// Construct a new editor with the provided text.
    pub fn new(txt: &str) -> Self {
        Self {
            state: NodeState::default(),
            view: EditorView::new(txt),
        }
    }

    /// Move the cursor left or right.
    #[command]
    fn cursor_shift(&mut self, _: &dyn Context, n: isize) {
        self.view.core.cursor_shift(n);
    }

    /// Move the cursor up or down in the chunk list.
    #[command]
    fn cursor_shift_chunk(&mut self, _: &dyn Context, n: isize) {
        self.view.core.cursor_shift_chunk(n);
    }

    /// Move the cursor up or down by visual line.
    #[command]
    fn cursor_shift_lines(&mut self, _: &dyn Context, n: isize) {
        self.view.core.cursor_shift_lines(n);
    }
}

// DefaultBindings is part of canopy, not canopy-core
// impl DefaultBindings for Editor {
//     fn defaults(b: Binder) -> Binder {
//         b.key(key::KeyCode::Left, "editor::cursor_shift(1)")
//             .key(key::KeyCode::Right, "editor::cursor_shift(-1)")
//             .key(key::KeyCode::Down, "editor::cursor_shift_lines(1)")
//             .key(key::KeyCode::Up, "editor::cursor_shift_lines(-1)")
//             .key('h', "editor::cursor_shift(-1)")
//             .key('l', "editor::cursor_shift(1)")
//             .key('j', "editor::cursor_shift_chunk(1)")
//             .key('k', "editor::cursor_shift_chunk(-1)")
//     }
// }

impl Node for Editor {
    fn accept_focus(&mut self) -> bool {
        false
    }

    fn layout(&mut self, l: &Layout, sz: Expanse) -> Result<()> {
        l.place(&mut self.view, sz.rect())?;
        let vp = self.view.vp();
        self.wrap(vp)?;
        Ok(())
    }

    fn children(&mut self, f: &mut dyn FnMut(&mut dyn Node) -> Result<()>) -> Result<()> {
        f(&mut self.view)
    }
}
