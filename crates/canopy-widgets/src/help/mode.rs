//! Help for a transient mode.
//!
//! While a transient mode waits for its key, a small framed panel in the
//! bottom right corner lists the keys that mode binds. The panel takes no
//! focus and no keys, so the mode still receives the next key.

use canopy::{
    ChildSlot, Context, ContextExt, NodeId, NodeName, Render, ViewContext, Widget,
    error::{Error, Result},
    geom::Size,
    layout::{Align, Constraint, Direction, Edges, Layout, MeasureConstraints, Measurement},
};

use super::binding_list::{BindingRow, display_lines, key_rows_of, natural_width, render_line};
use crate::frame::Frame;

canopy::slot!(ModeFrameSlot: Frame);
canopy::slot!(ModeBindingsSlot: ModeBindings);

/// Widest panel content in cells, including the side margins.
const MAX_WIDTH: u32 = 64;

/// Blank columns on each side of the rows.
const MARGIN: u32 = 1;

/// Bindings of one transient mode, measured to fit them.
///
/// A transient mode takes the next key, so the panel lists keys alone.
pub struct ModeBindings {
    /// Rows for the keys in the mode's own tier.
    bindings: Vec<BindingRow>,
}

impl ModeBindings {
    /// Construct an empty list.
    const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Return the width that shows every action on one row.
    fn preferred_width(&self) -> u32 {
        u32::try_from(natural_width(&self.bindings))
            .unwrap_or(u32::MAX)
            .saturating_add(2 * MARGIN)
            .min(MAX_WIDTH)
    }
}

impl Widget for ModeBindings {
    fn measure(&self, constraints: MeasureConstraints) -> Measurement {
        let preferred = self.preferred_width();
        let width = match constraints.width {
            Constraint::Exact(width) => width,
            Constraint::AtMost(width) => preferred.min(width),
            Constraint::Unbounded => preferred,
        };
        let rows = display_lines(&self.bindings, width.saturating_sub(2 * MARGIN)).len();
        constraints.clamp(Size::new(width, u32::try_from(rows).unwrap_or(u32::MAX)))
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        let rect = context.view().outer_rect_local();
        render.fill("help/panel", rect, ' ')?;
        let width = rect.w.saturating_sub(2 * MARGIN);
        let lines = display_lines(&self.bindings, width);
        for (y, line) in (0..rect.h).zip(&lines) {
            render_line(render, line, MARGIN, y, width)?;
        }
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("mode_bindings")
    }
}

/// Overlay that shows a transient mode's bindings in the bottom right corner
/// while it waits for its key.
///
/// The overlay is an end-aligned stack that pushes the `help` style layer its
/// framed bindings list paints into.
pub struct ModeHelp;

impl ModeHelp {
    /// Build the mode help subtree and return its root.
    pub(crate) fn install(context: &mut dyn Context) -> Result<NodeId> {
        let bindings = context.create_detached(ModeBindings::new())?;
        let frame = context.create_detached(Frame::new())?;
        context.attach_slot(frame.into(), ModeBindingsSlot::KEY, bindings.into())?;
        context.with_layout_of(frame.into(), &mut |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;

        let overlay = context.create_detached(Self)?;
        context.attach_slot(overlay.into(), ModeFrameSlot::KEY, frame.into())?;
        Ok(overlay.into())
    }

    /// Show the bindings of the transient mode that waits for a key, or hide
    /// the panel when no such mode is active.
    ///
    /// Contextual help replaces the panel while it is open.
    pub(crate) fn sync(context: &mut dyn Context, overlay: NodeId) -> Result<()> {
        let snapshot = context.available_bindings(context.focused_node())?;
        let Some(mode) = snapshot.transient_mode else {
            context.set_hidden_of(overlay, true)?;
            return Ok(());
        };
        let bindings = snapshot
            .bindings
            .into_iter()
            .filter(|binding| binding.tier.mode() == Some(mode.as_str()))
            .collect::<Vec<_>>();
        let bindings = key_rows_of(&bindings);

        let frame = context
            .get_slot_of::<ModeFrameSlot>(overlay)?
            .ok_or_else(|| Error::NotFound("mode help frame".into()))?;
        let list = context
            .get_slot_of::<ModeBindingsSlot>(NodeId::from(frame))?
            .ok_or_else(|| Error::NotFound("mode bindings".into()))?;
        context.with_widget_mut(frame, |frame: &mut Frame, _context| {
            frame.set_title(mode);
            Ok(())
        })?;
        context.with_widget_mut(list, |list: &mut ModeBindings, context| {
            list.bindings = bindings;
            context.invalidate_layout();
            Ok(())
        })?;
        context.set_hidden_of(overlay, false)?;
        Ok(())
    }
}

impl Widget for ModeHelp {
    fn layout(&self) -> Layout {
        Layout::fill()
            .direction(Direction::Stack)
            .align_horizontal(Align::End)
            .align_vertical(Align::End)
            .padding(Edges::all(1))
    }

    fn render(&mut self, render: &mut Render, _context: &dyn ViewContext) -> Result<()> {
        render.push_layer("help");
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("mode_help")
    }
}
