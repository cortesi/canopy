//! Help for a transient mode.
//!
//! While a transient mode waits for its key, a small dialog centred over the
//! view lists the keys that mode binds, the way every other modal panel sits,
//! and `Root` dims the application behind it as it does behind a modal. The
//! panel takes no focus and no keys, so the mode still receives the next key.

use canopy::{
    Context, ContextExt, NodeId, NodeName, ViewContext, ViewContextExt, Widget,
    commands::CommandStatus,
    error::{Error, Result},
    geom::Point,
    layout::{Layout, ScrollOp},
    render::Render,
};

use super::binding_list::{grouped_keys, key_rows_of};
use crate::{Dialog, MenuList, MenuRow};

/// Widest frame: the rows and the frame's two borders.
const MAX_FRAME_WIDTH: u32 = 66;

/// Overlay that shows a transient mode's bindings in a dialog centred over
/// the view while it waits for its key.
///
/// The dialog inside the overlay pushes the `dialog` layer, so the panel
/// reads as every modal panel does, and its rows are a menu's, without a
/// selection.
pub struct ModeHelp;

impl ModeHelp {
    /// Build the mode help subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let overlay = context.create_detached(Self)?;
        // A transient mode takes the next key, so its menu lists the keys
        // without a selection.
        Dialog::new().with_max_width(MAX_FRAME_WIDTH).add(
            context,
            overlay.into(),
            MenuList::new().with_selection(false),
        )?;
        Ok(overlay.into())
    }

    /// Show the bindings of the transient mode that waits for a key, or hide
    /// the panel when no such mode is active, and return whether it shows.
    ///
    /// Contextual help replaces the panel while it is open.
    pub(crate) fn sync(context: &mut dyn Context, overlay: NodeId) -> Result<Shown> {
        let Some(mode) = context.transient_mode() else {
            context.set_hidden(overlay, true)?;
            return Ok(Shown::Hidden);
        };
        // A menu lists only what its keys can do now. Help search reads the
        // menu through the same query, so the two always agree.
        let bindings = context
            .menu_bindings(context.focused_node(), &mode)?
            .into_iter()
            .filter(|binding| {
                !binding
                    .command
                    .as_ref()
                    .and_then(|command| command.availability.as_ref())
                    .is_some_and(|availability| {
                        matches!(availability.status, Some(CommandStatus::Disabled(_)))
                    })
            })
            .collect::<Vec<_>>();
        let mut rows: Vec<MenuRow> = grouped_keys(&key_rows_of(&bindings))
            .into_iter()
            .map(|(keys, description)| MenuRow::keyed(keys, description))
            .collect();
        if rows.is_empty() {
            rows.push(MenuRow::new(None, "No key bindings in this context"));
        }

        let dialog = context
            .unique_descendant::<Dialog>(overlay)?
            .ok_or_else(|| Error::NotFound("mode help dialog".into()))?;
        let list = context
            .unique_descendant::<MenuList>(overlay)?
            .ok_or_else(|| Error::NotFound("mode help bindings".into()))?;
        Dialog::set_title(context, dialog.into(), mode)?;
        context.update(list, |list: &mut MenuList| list.set_rows(rows))?;
        if !context.set_hidden(overlay, false)?.changed() {
            return Ok(Shown::Kept);
        }
        // More keys than rows scroll under the wheel, and each mode opens at
        // the top of its keys.
        context.scroll_node(list.into(), ScrollOp::To(Point::ZERO))?;
        Ok(Shown::Opened)
    }
}

/// Whether the mode help panel shows after a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// The panel was hidden and now shows.
    Opened,
    /// The panel showed and still does.
    Kept,
    /// The panel is hidden.
    Hidden,
}

impl Widget for ModeHelp {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("help");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("mode_help")
    }
}
