//! Rendering pipeline for the canopy facade.

use std::sync::mpsc;

use super::Canopy;
use crate::{
    NodeId,
    core::{context::CoreViewContext, termbuf::TermBuf, view::View, world::WidgetOperation},
    cursor,
    error::Result,
    event::Event,
    geom::{Point, Rect, RectI32, Size},
    layout::Display,
    render::{Render, RenderBackend},
    style::{Effect, ResolvedStyle, StyleManager},
};

/// Rendering traversal scratch state shared across recursion.
struct RenderTraversal<'a> {
    /// Destination buffer for draw operations.
    dest_buf: &'a mut TermBuf,
    /// Style manager stack.
    styl: &'a mut StyleManager,
    /// Accumulated style effects for the current subtree.
    effect_stack: &'a mut Vec<Effect>,
}

/// No-op backend used to refresh the offscreen terminal buffer for inspection.
struct SnapshotBackend;

impl RenderBackend for SnapshotBackend {
    fn style(&mut self, _style: &ResolvedStyle) -> Result<()> {
        Ok(())
    }

    fn text(&mut self, _loc: Point, _txt: &str) -> Result<()> {
        Ok(())
    }

    fn supports_char_shift(&self) -> bool {
        false
    }

    fn shift_chars(&mut self, _loc: Point, _count: i32) -> Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Canopy {
    /// Render the tree only if a render is pending.
    pub(crate) fn render_if_pending<R: RenderBackend>(&mut self, be: &mut R) -> Result<bool> {
        if !self.render_pending {
            return Ok(false);
        }
        self.render(be)?;
        Ok(true)
    }

    /// Refresh the cached terminal buffer without producing user-visible output.
    pub(crate) fn refresh_snapshot(&mut self) -> Result<()> {
        let mut backend = SnapshotBackend;
        let _ignored = self.render_if_pending(&mut backend)?;
        Ok(())
    }

    /// Has the focus path status of this node changed since the last render sweep?
    pub fn node_focus_path_changed(&self, node_id: impl Into<NodeId>) -> bool {
        let node_id = node_id.into();
        if self.focus_changed() {
            self.core.is_on_focus_path(node_id) || self.last_focus_path.contains(&node_id)
        } else {
            false
        }
    }

    /// Register the poller channel.
    pub(crate) fn start_poller(&mut self, tx: mpsc::Sender<Event>) {
        self.event_tx = tx;
    }

    /// Pre-render sweep of the tree.
    pub(crate) fn pre_render(&mut self) -> Result<bool> {
        let root = self.core.root;
        let mut focus_seen = false;
        let mut layout_dirty = false;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let hidden = self.core.nodes.get(id).map(|n| n.hidden).unwrap_or(false);
            if hidden {
                continue;
            }

            if self.core.is_focused(id) {
                focus_seen = true;
            }

            let mounted = self.core.nodes.get(id).map(|n| n.mounted).unwrap_or(false);
            if !mounted {
                layout_dirty = true;
                self.core.mount_node(id)?;
            }

            let initialized = self
                .core
                .nodes
                .get(id)
                .map(|n| n.initialized)
                .unwrap_or(false);
            if !initialized {
                layout_dirty = true;
                let next = self.core.with_widget_mut(id, |w, core| {
                    let mut ctx = crate::core::context::CoreContext::new(core, id);
                    w.poll(&mut ctx)
                })?;
                if let Some(d) = next {
                    self.poller.schedule(id, d);
                }
                if let Some(node) = self.core.nodes.get_mut(id) {
                    node.initialized = true;
                }
            }

            let children = self.core.nodes[id].children.clone();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        if !focus_seen {
            self.core.focus_first(root);
        }

        Ok(layout_dirty)
    }

    /// Render a single node (without children).
    fn render_node(
        &self,
        dest_buf: &mut TermBuf,
        styl: &mut StyleManager,
        node_id: NodeId,
        view: View,
        screen_clip: Rect,
        effect_slice: &[Effect],
    ) -> Result<()> {
        let local_clip = Self::outer_clip_to_local(view.outer, screen_clip);
        let screen_origin = screen_clip.tl;

        let mut rndr = Render::new_shared(&self.style, styl, dest_buf, local_clip, screen_origin)
            .with_effects(effect_slice);

        let result = self.core.with_widget_render(node_id, |widget, core| {
            let ctx = CoreViewContext::new(core, node_id);
            widget.render(&mut rndr, &ctx)
        })?;
        result.map_err(|error| {
            self.core
                .widget_operation_error(WidgetOperation::render("render"), node_id, &error)
        })
    }

    /// Recursively render a node subtree.
    fn render_recursive(
        &mut self,
        traversal: &mut RenderTraversal<'_>,
        node_id: NodeId,
        parent_clip: Rect,
        active_start: usize,
        active_len: usize,
    ) -> Result<()> {
        let (hidden, layout, view, children, clear_inherited) = {
            let node = &self.core.nodes[node_id];
            (
                node.hidden,
                node.layout,
                node.view,
                node.children.clone(),
                node.clear_inherited_effects,
            )
        };

        if hidden || layout.display == Display::None {
            return Ok(());
        }

        let Some(screen_clip) = view.outer.intersect_rect(parent_clip) else {
            return Ok(());
        };

        let saved_len = traversal.effect_stack.len();
        let (base_start, base_len) = if clear_inherited {
            (saved_len, 0)
        } else {
            (active_start, active_len)
        };

        if let Some(local) = self.core.nodes[node_id].effects.as_ref() {
            traversal.effect_stack.extend(local.iter().cloned());
        }

        let current_len = base_len + traversal.effect_stack.len() - saved_len;

        traversal.styl.push();

        {
            let effect_slice = &traversal.effect_stack[base_start..base_start + current_len];
            self.render_node(
                traversal.dest_buf,
                traversal.styl,
                node_id,
                view,
                screen_clip,
                effect_slice,
            )?;
        }

        if let Some(children_clip) = view.content.intersect_rect(parent_clip) {
            for child in children {
                self.render_recursive(traversal, child, children_clip, base_start, current_len)?;
            }
        }

        traversal.styl.pop();
        traversal.effect_stack.truncate(saved_len);

        Ok(())
    }

    /// Render the tree into an offscreen buffer.
    fn render_pass(&mut self, root_size: Size) -> Result<TermBuf> {
        let mut styl = StyleManager::default();
        styl.reset();

        let def_style = styl
            .get(&self.style, "")
            .resolve_solid()
            .expect("default style resolves to solid colors");
        let mut next = TermBuf::new(root_size, ' ', def_style);

        let screen_clip = Rect::new(0, 0, root_size.w, root_size.h);
        let mut effect_stack: Vec<Effect> = Vec::new();
        let mut traversal = RenderTraversal {
            dest_buf: &mut next,
            styl: &mut styl,
            effect_stack: &mut effect_stack,
        };
        self.render_recursive(&mut traversal, self.core.root, screen_clip, 0, 0)?;
        self.post_render(&mut next)?;

        Ok(next)
    }

    /// Post-render sweep of the tree.
    pub(crate) fn post_render(&self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(NodeId, View, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor =
                self.core
                    .with_widget_read(id, WidgetOperation::render("cursor"), |w, _| w.cursor())?;
            if let Some(node_cursor) = cursor
                && let Some(node) = self.core.nodes.get(id)
            {
                cursor_spec = Some((id, node.view, node_cursor));
                break;
            }
            current = self.core.nodes.get(id).and_then(|n| n.parent);
        }

        if let Some((_nid, view, c)) = cursor_spec {
            let view_rect = Rect::new(0, 0, view.content.w, view.content.h);
            if view_rect.contains_point(c.location) {
                let screen_x = view.content.tl.x + c.location.x as i32;
                let screen_y = view.content.tl.y + c.location.y as i32;
                if screen_x >= 0 && screen_y >= 0 {
                    let screen_pos = Point {
                        x: screen_x as u32,
                        y: screen_y as u32,
                    };
                    buf.overlay_cursor(screen_pos, c.shape);
                }
            }
        }

        Ok(())
    }

    /// Render the widget tree. All visible nodes are rendered.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        let first_render = self.termbuf.is_none();

        // Apply pending style change from Context::set_style
        if let Some(new_style) = self.core.pending_style.take() {
            self.style = new_style;
        }

        if let Some(root_size) = self.root_size {
            self.core.update_layout(root_size)?;

            let layout_dirty = self.pre_render()?;
            if layout_dirty {
                self.core.update_layout(root_size)?;
            }

            let _ = self.core.take_help_snapshot_observed();
            let mut next = self.render_pass(root_size)?;
            if self.core.take_help_snapshot_observed() {
                self.core.pending_help_snapshot = None;
                self.core.update_layout(root_size)?;
                if layout_dirty {
                    self.core.update_layout(root_size)?;
                }
                next = self.render_pass(root_size)?;
            }

            be.reset()?;

            if let Some(prev) = &self.termbuf {
                next.diff(prev, be)?;
            } else {
                next.render(be)?;
            }
            self.termbuf = Some(next);

            if let Some(target) = self.core.take_diagnostic_dump_request() {
                eprintln!("{}", self.diagnostic_dump(target));
            }

            self.last_render_focus_gen = self.core.focus_gen;
            self.last_focus_path = self.core.focus_path_ids();

            if first_render && self.run_on_start_hooks()? {
                return self.render(be);
            }

            self.render_pending = false;
        }

        Ok(())
    }

    /// Convert a screen-space clip rect into local outer coordinates.
    fn outer_clip_to_local(outer: RectI32, clip: Rect) -> Rect {
        let dx = (clip.tl.x as i64 - outer.tl.x as i64).max(0) as u32;
        let dy = (clip.tl.y as i64 - outer.tl.y as i64).max(0) as u32;
        Rect::new(dx, dy, clip.w, clip.h)
    }
}
