//! Rendering pipeline for the canopy facade.

use super::Canopy;
use crate::{
    NodeId,
    core::{
        context::CoreViewContext, termbuf::TermBuf, view::View, wake::WorkStamp,
        world::WidgetOperation,
    },
    cursor,
    error::{Error, Result},
    geom::{Point, Rect, Size},
    layout::Display,
    render::{Render, RenderBackend},
    style::{Effect, StyleManager},
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

impl Canopy {
    /// Render the tree only if a render is pending.
    #[cfg(test)]
    pub(crate) fn render_if_pending<R: RenderBackend>(&mut self, be: &mut R) -> Result<bool> {
        if !self.render_pending && !self.core.changes.is_pending() {
            return Ok(false);
        }
        self.render(be)?;
        Ok(true)
    }

    /// Refresh observation data without advancing terminal output history.
    pub(crate) fn refresh_snapshot(&mut self) -> Result<()> {
        self.prepare_frame(false).map(|_| ())
    }

    /// Poll one node and schedule its next callback.
    pub(crate) fn poll_node(&mut self, node_id: NodeId) -> Result<()> {
        let entry = self
            .core
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        let attachment = match entry.poll_lifetime {
            crate::WorkLifetime::Node => None,
            crate::WorkLifetime::Attachment => {
                let Some(generation) = entry.attachment_generation else {
                    return Ok(());
                };
                Some(generation)
            }
        };
        let stamp = WorkStamp {
            node: node_id,
            incarnation: entry.incarnation,
            attachment,
        };
        let checkpoint = self.core.begin_dispatch();
        let result = self
            .core
            .with_widget_ctx(node_id, |widget, ctx| widget.poll(ctx));
        let completion = self.core.finish_dispatch(checkpoint, result.is_ok());
        let next = result?;
        completion?;
        if self.core.work_stamp_valid(stamp)
            && let Some(next) = next
        {
            self.poller.schedule(stamp, next)?;
        }
        Ok(())
    }

    /// Pre-render sweep of the tree.
    fn pre_render(&mut self) -> Result<bool> {
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
                self.poll_node(id)?;
                if let Some(node) = self.core.nodes.get_mut(id) {
                    node.initialized = true;
                }
            }

            let Some(node) = self.core.nodes.get(id) else {
                continue;
            };
            let children = node.children.clone();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }

        if !focus_seen {
            self.core.focus_first(root)?;
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
        let local = view.outer.to_local_point(screen_clip.tl);
        let local_clip = Rect::new(local.x, local.y, screen_clip.w, screen_clip.h);
        let screen_origin = screen_clip.tl;

        let mut rndr = Render::new(&self.style, styl, dest_buf, local_clip, screen_origin)
            .with_effects(effect_slice);

        let result = self.core.with_widget_render(node_id, |widget, core| {
            let ctx = CoreViewContext::new(core, node_id);
            widget.render(&mut rndr, &ctx)
        })?;
        result.map_err(|error| {
            self.core
                .widget_operation_error(WidgetOperation::render("render"), node_id, error)
        })
    }

    /// Recursively render a node subtree.
    fn render_recursive(
        &self,
        traversal: &mut RenderTraversal<'_>,
        node_id: NodeId,
        parent_clip: Rect,
        active_start: usize,
        active_len: usize,
    ) -> Result<()> {
        let node = &self.core.nodes[node_id];

        if node.hidden || node.layout.display == Display::None {
            return Ok(());
        }

        let view = node.view;
        let Some(screen_clip) = view.outer.intersect_rect(parent_clip) else {
            return Ok(());
        };

        let saved_len = traversal.effect_stack.len();

        traversal.effect_stack.extend(node.effects.iter().cloned());
        traversal
            .effect_stack
            .extend(self.core.modal_effects_for(node_id));

        let current_len = active_len + traversal.effect_stack.len() - saved_len;

        traversal.styl.push();

        {
            let effect_slice = &traversal.effect_stack[active_start..active_start + current_len];
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
            for child in &node.children {
                self.render_recursive(traversal, *child, children_clip, active_start, current_len)?;
            }
        }

        traversal.styl.pop();
        traversal.effect_stack.truncate(saved_len);

        Ok(())
    }

    /// Render the tree into an offscreen buffer.
    fn render_pass(&self, root_size: Size) -> Result<TermBuf> {
        let mut styl = StyleManager::default();

        let def_style = styl
            .get(&self.style, "")
            .resolve_solid()
            .expect("default style resolves to solid colors");
        let mut next = TermBuf::new_with_limits(root_size, ' ', def_style, self.render_limits)?;

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
    fn post_render(&self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(View, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor =
                self.core
                    .with_widget_read(id, WidgetOperation::render("cursor"), |w, _| w.cursor())?;
            if let Some(node_cursor) = cursor
                && let Some(node) = self.core.nodes.get(id)
            {
                cursor_spec = Some((node.view, node_cursor));
                break;
            }
            current = self.core.nodes.get(id).and_then(|n| n.parent);
        }

        if let Some((view, c)) = cursor_spec {
            let view_rect = Rect::new(0, 0, view.content.w, view.content.h);
            if view_rect.contains_point(c.location) {
                let screen_x = i64::from(view.content.tl.x) + i64::from(c.location.x);
                let screen_y = i64::from(view.content.tl.y) + i64::from(c.location.y);
                if let (Ok(x), Ok(y)) = (u32::try_from(screen_x), u32::try_from(screen_y)) {
                    let screen_pos = Point { x, y };
                    buf.overlay_cursor(screen_pos, c.shape);
                }
            }
        }

        Ok(())
    }

    /// Prepare and publish pending state without writing to a backend.
    pub(super) fn prepare_frame(&mut self, force: bool) -> Result<bool> {
        if !self.driver.startup_attempted {
            self.run_startup_scripts()?;
        }
        if !force
            && !self.render_pending
            && !self.core.changes.is_pending()
            && !self.script_host.has_on_start_hooks()
        {
            return Ok(false);
        }
        let Some(root_size) = self.root_size else {
            return Ok(false);
        };
        if let Some(new_style) = self.core.pending_style.take() {
            self.style = new_style;
        }
        self.pre_render()?;
        self.core.update_layout(root_size)?;
        if self.run_on_start_hooks()? {
            self.pre_render()?;
            self.core.update_layout(root_size)?;
        }
        let next = self.render_pass(root_size)?;
        self.termbuf = Some(next);
        self.render_pending = false;
        self.core.changes = crate::ChangeSet::default();
        self.driver.publication.publish();
        if let Some(target) = self.core.take_diagnostic_dump_request() {
            eprintln!("{}", self.diagnostic_dump(target));
        }
        Ok(true)
    }

    /// Emit published cells. Failed writes preserve the last successful
    /// baseline.
    pub(crate) fn emit_frame<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        let Some(next) = &self.termbuf else {
            return Ok(());
        };
        be.reset()?;
        if let Some(previous) = &self.emitted_buf {
            next.diff(previous, be)?;
        } else {
            next.render(be)?;
        }
        be.flush()?;
        self.emitted_buf = Some(next.clone());
        Ok(())
    }

    /// Prepare and render the widget tree explicitly.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        self.prepare_frame(true)?;
        self.emit_frame(be)
    }
}
