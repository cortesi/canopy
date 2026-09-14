//! Help for a transient input mode.
//!
//! While a transient mode waits for its key, a small framed panel in the
//! bottom right corner lists the keys that mode binds. The panel takes no
//! focus and no keys, so the mode still receives the next key.

use canopy::{
    ChildSlot, Context, ContextExt, NodeId, NodeName, Render, ViewContext, Widget,
    error::{Error, Result},
    geom::Size,
    help::AvailableBinding,
    layout::{Align, Constraint, Direction, Edges, Layout, MeasureConstraints, Measurement},
};
use unicode_width::UnicodeWidthStr;

use super::binding_list::{binding_description, display_lines, render_line};
use crate::{center::Center, frame::Frame};

canopy::slot!(ModeFrameSlot: Frame);
canopy::slot!(ModeBindingsSlot: ModeBindings);

/// Widest panel content in cells, including the side margins.
const MAX_WIDTH: u32 = 64;

/// Blank columns on each side of the rows.
const MARGIN: u32 = 1;

/// Bindings of one transient mode, measured to fit them.
pub struct ModeBindings {
    /// Bindings in the mode's own scope.
    bindings: Vec<AvailableBinding>,
}

impl ModeBindings {
    /// Construct an empty list.
    const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Return the width that shows every binding on one row.
    fn preferred_width(&self) -> u32 {
        let widest = |text: &dyn Fn(&AvailableBinding) -> String| {
            self.bindings
                .iter()
                .map(|binding| UnicodeWidthStr::width(text(binding).as_str()))
                .max()
                .unwrap_or(0)
        };
        let key = widest(&|binding| binding.key.to_string());
        let description = widest(&binding_description);
        u32::try_from(key + 2 + description)
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
        render.push_layer("help");
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

/// Panel listing the keys of a transient mode while it waits.
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

        let overlay = context.create_detached(Center::new())?;
        context.attach_slot(overlay.into(), ModeFrameSlot::KEY, frame.into())?;
        context.with_layout_of(overlay.into(), &mut |layout| {
            *layout = Layout::fill()
                .direction(Direction::Stack)
                .align_horizontal(Align::End)
                .align_vertical(Align::End)
                .padding(Edges::all(1));
        })?;
        Ok(overlay.into())
    }

    /// Show the bindings of the transient mode that waits for a key, or hide
    /// the panel when no such mode is active.
    ///
    /// Contextual help replaces the panel while it is open.
    pub(crate) fn sync(context: &mut dyn Context, overlay: NodeId) -> Result<()> {
        let snapshot = context.available_bindings(context.focused_node())?;
        let mode = snapshot
            .transient_mode
            .filter(|_| snapshot.exclusive_group.is_none());
        let Some(mode) = mode else {
            context.set_hidden_of(overlay, true)?;
            return Ok(());
        };
        let bindings = snapshot
            .bindings
            .into_iter()
            .filter(|binding| binding.scope.mode() == Some(mode.as_str()))
            .collect::<Vec<_>>();

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
