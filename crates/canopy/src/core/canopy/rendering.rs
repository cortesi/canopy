//! Rendering pipeline for the canopy facade.

use std::sync::Arc;

use super::{Canopy, FrameId, motion::PrimaryCursor};
use crate::{
    NodeId,
    core::{
        context::CoreViewContext,
        cursor::{CursorMotion, CursorSnapshot},
        notice::NoticeSource,
        render::{DeclaredCursor, RenderFrame},
        snapshot,
        termbuf::TermBuf,
        view::View,
        world::WidgetOperation,
    },
    error::{Error, Result},
    geom::{Point, Rect, Size},
    render::{Render, RenderBackend},
    style::{
        AttrSet, Color, MotionClocks, Paint, Style, StyleChange, StyleManager, effects::Effect,
    },
};

/// Rendering traversal scratch state shared across recursion.
struct RenderTraversal<'a> {
    /// Destination buffer for draw operations.
    dest_buf: &'a mut TermBuf,
    /// Style manager stack.
    styl: &'a mut StyleManager,
    /// Accumulated style effects for the current subtree.
    effect_stack: &'a mut Vec<Effect>,
    /// Cursors declared so far.
    cursors: &'a mut Vec<DeclaredCursor>,
    /// Clocks of the frame.
    clocks: MotionClocks,
}

/// Apply a node's style effects to a cursor color, at rest.
fn effect_color(effects: &[Effect], color: Color) -> Color {
    let style = Style {
        fg: Paint::Solid(color),
        bg: Paint::Solid(color),
        attrs: AttrSet::default(),
    };
    effects
        .iter()
        .fold(style, |style, effect| effect.apply(style))
        .fg
        .resolve(Rect::new(0, 0, 1, 1), Point::ZERO)
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
        let sources = self.core.push_notice_source(NoticeSource::Poll);
        let result = self.with_dispatch_boundary(|canopy| {
            canopy
                .core
                .with_widget_ctx(node_id, |widget, ctx| widget.poll(ctx))?
        });
        self.core.pop_notice_source(sources);
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
            // A mount's posted calls and a first poll can remove nodes the
            // sweep has already stacked.
            if !self.core.nodes.contains_key(id) {
                continue;
            }
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
                // The mount is its own boundary, so the calls its hook posts
                // run now, and may remove the node before the sweep polls it.
                self.with_notice_boundary(NoticeSource::Widget, Some(id), |canopy| {
                    canopy.core.mount_node(id)
                })?;
                if !self.core.nodes.contains_key(id) {
                    continue;
                }
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
        frame: RenderFrame<'_>,
        view: View,
        screen_clip: Rect,
        effect_slice: &[Effect],
    ) -> Result<()> {
        let node_id = frame.node;
        let local = view.outer.to_local_point(screen_clip.tl);
        let local_clip = Rect::new(local.x, local.y, screen_clip.w, screen_clip.h);
        let screen_origin = screen_clip.tl;

        let mut rndr = Render::new(&self.style, styl, dest_buf, local_clip, screen_origin)
            .with_effects(effect_slice)
            .with_frame(frame);

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
            let frame = RenderFrame {
                node: node_id,
                clocks: traversal.clocks,
                cursors: traversal.cursors,
            };
            self.render_node(
                traversal.dest_buf,
                traversal.styl,
                frame,
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

    /// Render the tree into an offscreen buffer, with its cursors painted.
    fn render_pass(
        &self,
        screen_size: Size,
        clocks: MotionClocks,
    ) -> Result<(TermBuf, Vec<CursorSnapshot>)> {
        let mut styl = StyleManager::default();

        let def_style = styl
            .get(&self.style, "")
            .resolve_solid()
            .expect("default style resolves to solid colors");
        let mut next =
            TermBuf::new_with_limits(screen_size, ' ', def_style, self.frame.render_limits)?;

        let screen_clip = Rect::new(0, 0, screen_size.w, screen_size.h);
        let mut effect_stack: Vec<Effect> = Vec::new();
        let mut cursors = Vec::new();
        let mut traversal = RenderTraversal {
            dest_buf: &mut next,
            styl: &mut styl,
            effect_stack: &mut effect_stack,
            cursors: &mut cursors,
            clocks,
        };
        self.render_recursive(&mut traversal, self.core.root, screen_clip, 0, 0)?;
        let cursors = self.paint_cursors(&mut next, cursors);

        Ok((next, cursors))
    }

    /// Paint the declared cursors over the frame, secondary cursors first.
    ///
    /// The primary cursor is the declaration of the deepest node on the focus
    /// path. It takes the motion of its look. Every other cursor is steady.
    fn paint_cursors(
        &self,
        buf: &mut TermBuf,
        declared: Vec<DeclaredCursor>,
    ) -> Vec<CursorSnapshot> {
        let mut primary = None;
        let mut current = self.core.focus;
        while let Some(id) = current {
            if declared.iter().any(|cursor| cursor.node == id) {
                primary = Some(id);
                break;
            }
            current = self.core.nodes.get(id).and_then(|node| node.parent);
        }
        let (first, secondary): (Vec<_>, Vec<_>) = declared
            .into_iter()
            .partition(|cursor| Some(cursor.node) == primary);
        let mut snapshots = Vec::new();
        for cursor in secondary.into_iter().chain(first) {
            let is_primary = Some(cursor.node) == primary;
            let mut look = cursor
                .request
                .apply(self.cursor_looks.resolve(&cursor.request.role));
            look.color = effect_color(&cursor.effects, look.color);
            if !is_primary {
                look.motion = CursorMotion::Steady;
            }
            let Some(paint) = buf.paint_cursor(cursor.location, look.shape, look.color) else {
                continue;
            };
            if is_primary {
                buf.move_cursor(paint, look.motion);
            }
            snapshots.push(CursorSnapshot {
                node: cursor.node,
                location: cursor.location,
                role: cursor.request.role.into_owned(),
                look,
                primary: is_primary,
            });
        }
        // The primary cursor leads, then the rest in render order.
        if let Some(last) = snapshots.pop_if(|cursor| cursor.primary) {
            snapshots.insert(0, last);
        }
        snapshots
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
        let clocks = self.motion_clocks();
        let (next, cursors) = self.render_pass(screen_size, clocks)?;
        let frame_id = FrameId(self.driver.publication.generation() + 1);
        let primary = cursors
            .first()
            .filter(|cursor| cursor.primary)
            .map(|cursor| PrimaryCursor {
                node: cursor.node,
                location: cursor.location,
                role: cursor.role.clone(),
            });
        let snapshot = snapshot::capture(&self.core, frame_id, Arc::new(next), cursors)?;
        self.frame.snapshot = Some(Arc::new(snapshot));
        self.motion.primary(primary, clocks.now);
        self.core.changes = crate::ChangeSet::default();
        self.driver.publication.publish();
        Ok(true)
    }

    /// Emit published cells. After a backend failure, repaint the next frame
    /// in full because some output may already have reached the terminal.
    ///
    /// Emission writes the current colors of moving cells over the published
    /// buffer, which stays at rest. The composed buffer becomes the last
    /// emitted buffer, so the next emission diffs against the screen.
    pub(crate) fn emit_frame<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        let Some(snapshot) = self.frame.snapshot.clone() else {
            return Ok(());
        };
        let now = self.now();
        let next = if self.motion.active() && snapshot.buffer.has_motion() {
            let clocks = self.motion.clocks(now);
            let mut composed = (*snapshot.buffer).clone();
            composed.apply_motion(&clocks);
            Arc::new(composed)
        } else {
            Arc::clone(&snapshot.buffer)
        };
        let previous = self.frame.emitted_buf.take();
        be.reset()?;
        if let Some(previous) = previous {
            next.emit_diff(&previous, be)?;
        } else {
            next.emit(be)?;
        }
        let parked = snapshot
            .cursors
            .iter()
            .find(|cursor| cursor.primary)
            .map(|cursor| cursor.location);
        be.park_cursor(parked)?;
        be.flush()?;
        self.frame.emitted_buf = Some(next);
        self.schedule_motion(now);
        Ok(())
    }

    /// Prepare and render the widget tree explicitly.
    pub fn render<R: RenderBackend>(&mut self, be: &mut R) -> Result<()> {
        self.prepare_frame(true)?;
        self.emit_frame(be)
    }
}
