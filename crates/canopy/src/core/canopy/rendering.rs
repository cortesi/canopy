//! Rendering pipeline for the canopy facade.

use std::sync::Arc;

use super::{Canopy, FrameId};
use crate::{
    NodeId,
    core::{
        context::CoreViewContext, notice::NoticeSource, snapshot, termbuf::TermBuf, view::View,
        world::WidgetOperation,
    },
    error::{Error, Result},
    geom::{Point, Rect, Size},
    render::{Render, RenderBackend, cursor},
    style::{StyleChange, StyleManager, effects::Effect},
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
    /// Poll one node and schedule its next callback.
    pub(crate) fn poll_node(&mut self, node_id: NodeId) -> Result<()> {
        let lifetime = self
            .core
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?
            .poll_lifetime;
        let Some(stamp) = self.core.work_stamp(node_id, lifetime)? else {
            return Ok(());
        };
        // An explicit wake can arrive before the existing timer. Consume that
        // timer too, so the callback's return value decides all future polling.
        self.poller.cancel_owner(stamp.node, stamp.incarnation);
        let result = self.with_dispatch_boundary(|canopy| {
            canopy
                .core
                .with_widget_ctx(node_id, |widget, ctx| widget.poll(ctx))?
        });
        let next = match result {
            Ok(next) => {
                self.poller.set_interval(stamp, next);
                next
            }
            // A failed poll keeps its cadence, so work it retries runs again.
            Err(error) => {
                self.notice_or_fail(error, NoticeSource::Poll, Some(node_id))?;
                self.poller.interval(stamp)
            }
        };
        if self.core.work_stamp_valid(stamp)
            && let Some(next) = next
        {
            self.poller.schedule(stamp, next)?;
        }
        Ok(())
    }

    /// Pre-render sweep of the tree.
    fn mount_pending(&mut self) -> Result<bool> {
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

        if node.hidden {
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
    fn render_pass(&self, screen_size: Size) -> Result<TermBuf> {
        let mut styl = StyleManager::default();

        let def_style = styl
            .get(&self.style, "")
            .resolve_solid()
            .expect("default style resolves to solid colors");
        let mut next =
            TermBuf::new_with_limits(screen_size, ' ', def_style, self.frame.render_limits)?;

        let screen_clip = Rect::new(0, 0, screen_size.w, screen_size.h);
        let mut effect_stack: Vec<Effect> = Vec::new();
        let mut traversal = RenderTraversal {
            dest_buf: &mut next,
            styl: &mut styl,
            effect_stack: &mut effect_stack,
        };
        self.render_recursive(&mut traversal, self.core.root, screen_clip, 0, 0)?;
        self.overlay_cursor(&mut next)?;

        Ok(next)
    }

    /// Post-render sweep of the tree.
    fn overlay_cursor(&self, buf: &mut TermBuf) -> Result<()> {
        let mut current = self.core.focus;
        let mut cursor_spec: Option<(View, cursor::Cursor)> = None;
        while let Some(id) = current {
            let cursor = self
                .core
                .with_widget(id, WidgetOperation::render("cursor"), |w, _| w.cursor())?;
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

    /// Bring geometry up to date without painting, so the input routed next
    /// hit-tests the current tree.
    pub(super) fn settle_layout(&mut self) -> Result<()> {
        let Some(screen_size) = self.frame.screen_size else {
            return Ok(());
        };
        if !self.core.changes.layout_pending() {
            return Ok(());
        }
        self.mount_pending()?;
        self.core.update_layout(screen_size)
    }

    /// Prepare and publish pending state without writing to a backend.
    pub(super) fn prepare_frame(&mut self, force: bool) -> Result<bool> {
        if !self.driver.startup_attempted {
            self.run_startup_scripts()?;
        }
        if !force
            && !self.core.changes.is_pending()
            && !self.script.host.has_on_start_hooks()
            && !self.state_hooks_pending()
        {
            return Ok(false);
        }
        let Some(screen_size) = self.frame.screen_size else {
            return Ok(false);
        };
        match self.core.pending_style.take() {
            Some(StyleChange::Theme(palette)) => self.set_theme(palette),
            Some(StyleChange::Map(style)) => self.style = style,
            None => {}
        }
        self.run_state_hooks()?;
        self.mount_pending()?;
        // A first poll during the sweep can record a notice, which its hooks
        // show in this frame.
        if self.state_hooks_pending() {
            self.run_state_hooks()?;
            self.mount_pending()?;
        }
        self.core.update_layout(screen_size)?;
        if self.run_on_start_hooks()? {
            self.mount_pending()?;
            self.core.update_layout(screen_size)?;
        }
        let next = self.render_pass(screen_size)?;
        let frame_id = FrameId(self.driver.publication.generation() + 1);
        let snapshot = snapshot::capture(&self.core, frame_id, Arc::new(next))?;
        self.frame.snapshot = Some(Arc::new(snapshot));
        self.core.changes = crate::ChangeSet::default();
        self.driver.publication.publish();
        Ok(true)
    }

    /// Emit published cells. After a backend failure, repaint the next frame
    /// in full because some output may already have reached the terminal.
    pub(crate) fn emit_frame<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        let Some(next) = self.frame.snapshot.as_ref().map(|s| Arc::clone(&s.buffer)) else {
            return Ok(());
        };
        let previous = self.frame.emitted_buf.take();
        be.reset()?;
        if let Some(previous) = previous {
            next.emit_diff(&previous, be)?;
        } else {
            next.emit(be)?;
        }
        be.flush()?;
        self.frame.emitted_buf = Some(next);
        Ok(())
    }

    /// Prepare and render the widget tree explicitly.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        self.prepare_frame(true)?;
        self.emit_frame(be)
    }
}
