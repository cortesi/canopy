//! Keys for the editor tests: tokens in Vim notation, and one application
//! whose root context runs them with no render.

use std::cell::RefCell;

use canopy::Canopy;

use super::*;

thread_local! {
    /// One application gives the context of every run on a thread, since
    /// building one costs more than the keys.
    static APP: RefCell<Canopy> =
        RefCell::new(CanopyBuilder::new().build().expect("an empty application builds"));
}

/// Run `f` in the root context of the thread's application.
pub(super) fn in_root<R>(f: impl FnOnce(&mut dyn Context) -> Result<R>) -> R {
    APP.with(|app| app.borrow_mut().with_root_context(f))
        .expect("the keys run")
}

/// One step of a run: a key, or an undo or a redo that the host calls.
#[derive(Debug, Clone, Copy)]
pub(super) enum Step {
    /// A key.
    Key(key::Key),
    /// The host calls [`Editor::undo`].
    Undo,
    /// The host calls [`Editor::redo`].
    Redo,
}

/// Split `keys` into tokens: each char, or a name in angle brackets.
pub(super) fn tokens(keys: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = keys.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' && chars.peek().is_some_and(char::is_ascii_uppercase) {
            let mut name = String::from("<");
            for next in chars.by_ref() {
                name.push(next);
                if next == '>' {
                    break;
                }
            }
            out.push(name);
        } else {
            out.push(c.to_string());
        }
    }
    out
}

/// Return the step of one token: a char, the name of a key in angle
/// brackets, or `<Undo>` and `<Redo>` for calls of the host.
pub(super) fn step(token: &str) -> Step {
    Step::Key(match token {
        "<Undo>" => return Step::Undo,
        "<Redo>" => return Step::Redo,
        "<Esc>" => key::KeyCode::Esc.into(),
        "<CR>" => key::KeyCode::Enter.into(),
        "<BS>" => key::KeyCode::Backspace.into(),
        "<Del>" => key::KeyCode::Delete.into(),
        "<Left>" => key::KeyCode::Left.into(),
        "<Right>" => key::KeyCode::Right.into(),
        "<Up>" => key::KeyCode::Up.into(),
        "<Down>" => key::KeyCode::Down.into(),
        "<C-r>" => key::Ctrl + 'r',
        _ => {
            let mut chars = token.chars();
            let key = chars.next().expect("a token").into();
            assert!(chars.next().is_none(), "unknown token {token}");
            key
        }
    })
}

/// Run one step on `editor`.
pub(super) fn run_step(editor: &mut Editor, step: Step, ctx: &mut dyn Context) -> Result<()> {
    match step {
        Step::Key(key) => {
            editor.on_event(&Event::Key(key), ctx)?;
        }
        Step::Undo => editor.undo(ctx),
        Step::Redo => editor.redo(ctx),
    }
    Ok(())
}

/// Run `keys` on `editor` in `ctx`.
pub(super) fn run_keys(editor: &mut Editor, keys: &str, ctx: &mut dyn Context) -> Result<()> {
    for token in tokens(keys) {
        run_step(editor, step(&token), ctx)?;
    }
    Ok(())
}

/// Run `keys` on `editor`, with no render.
pub(super) fn drive(editor: &mut Editor, keys: &str) {
    in_root(|ctx| run_keys(editor, keys, ctx));
}
