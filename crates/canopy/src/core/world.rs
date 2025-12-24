use std::time::Duration;

use scoped_tls::scoped_thread_local;
use slotmap::SlotMap;
use taffy::{
    Taffy,
    error::TaffyError,
    geometry::Size,
    node::{MeasureFunc, Node as TaffyNode},
    style::{AvailableSpace, Dimension, Style},
};

use crate::{
    Context, ViewContext,
    backend::BackendControl,
    commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    core::{
        builder::NodeBuilder, context::CoreContext, id::NodeId, node::Node, viewport::ViewPort,
        viewstack::ViewStack,
    },
    error::{Error, Result},
    event::Event,
    geom::{Direction, Expanse, Point, Rect},
    path::Path,
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
};

scoped_thread_local!(static MEASURE_CONTEXT: *const ());

/// Context for Taffy measure callbacks.
struct MeasureContext<'a> {
    /// Node map used to locate widgets.
    nodes: &'a SlotMap<NodeId, Node>,
}

/// Resolve the measure function for a node if present.
fn measure_for_node(
    node_id: NodeId,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
) -> Size<f32> {
    MEASURE_CONTEXT.with(|ctx| {
        let ctx = unsafe { &*(*ctx as *const MeasureContext<'_>) };
        ctx.nodes
            .get(node_id)
            .and_then(|node| node.widget.as_ref())
            .map(|widget| widget.measure(known_dimensions, available_space))
            .unwrap_or(Size {
                width: 0.0,
                height: 0.0,
            })
    })
}

/// Core state for the arena, layout engine, and focus.
pub struct Core {
    /// Node storage arena.
    pub nodes: SlotMap<NodeId, Node>,
    /// Taffy layout tree.
    pub taffy: Taffy,
    /// Root node ID.
    pub root: NodeId,
    /// Currently focused node.
    pub focus: Option<NodeId>,
    /// Focus generation counter.
    pub focus_gen: u64,
    /// Active backend controller.
    pub backend: Option<Box<dyn BackendControl>>,
}

impl Core {
    /// Create a new Core with a default root node.
    pub fn new() -> Self {
        let mut taffy = Taffy::new();
        let mut nodes = SlotMap::with_key();
        let mut style = Style::default();
        let root_widget = RootContainer;
        root_widget.configure_style(&mut style);
        let taffy_id = taffy
            .new_leaf(style.clone())
            .expect("Failed to create root taffy node");
        let root_name = root_widget.name();
        let root = nodes.insert(Node {
            widget: Some(Box::new(root_widget)),
            parent: None,
            children: Vec::new(),
            taffy_id,
            style,
            viewport: Rect::zero(),
            vp: ViewPort::default(),
            hidden: false,
            name: root_name,
            initialized: false,
        });
        taffy
            .set_measure(
                taffy_id,
                Some(MeasureFunc::Boxed(Box::new(move |known, available| {
                    measure_for_node(root, known, available)
                }))),
            )
            .expect("Failed to register root measure function");

        Self {
            nodes,
            taffy,
            root,
            focus: None,
            focus_gen: 1,
            backend: None,
        }
    }

    /// Add a widget to the arena and return its node ID.
    pub fn add<W>(&mut self, widget: W) -> NodeId
    where
        W: Widget + 'static,
    {
        self.add_boxed(Box::new(widget))
    }

    /// Add a boxed widget to the arena and return its node ID.
    pub fn add_boxed(&mut self, widget: Box<dyn Widget>) -> NodeId {
        let mut style = Style::default();
        widget.configure_style(&mut style);
        let taffy_id = self
            .taffy
            .new_leaf(style.clone())
            .expect("Failed to create taffy node");

        let name = widget.name();
        let node_id = self.nodes.insert(Node {
            widget: Some(widget),
            parent: None,
            children: Vec::new(),
            taffy_id,
            style,
            viewport: Rect::zero(),
            vp: ViewPort::default(),
            hidden: false,
            name,
            initialized: false,
        });

        self.taffy
            .set_measure(
                taffy_id,
                Some(MeasureFunc::Boxed(Box::new(move |known, available| {
                    measure_for_node(node_id, known, available)
                }))),
            )
            .expect("Failed to register measure function");

        node_id
    }

    /// Replace the widget stored at a node.
    pub fn set_widget<W>(&mut self, node_id: NodeId, widget: W)
    where
        W: Widget + 'static,
    {
        let name = widget.name();
        let mut style = self
            .nodes
            .get(node_id)
            .map(|node| node.style.clone())
            .unwrap_or_default();
        widget.configure_style(&mut style);
        let node = self.nodes.get_mut(node_id).expect("Unknown node id");
        node.widget = Some(Box::new(widget));
        node.name = name;
        node.style = style.clone();

        let taffy_id = node.taffy_id;
        self.taffy
            .set_style(taffy_id, style)
            .map_err(|err| map_taffy_error(&err))
            .expect("Failed to set taffy style");
    }

    /// Mount a child under a parent in both the arena and Taffy.
    pub fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        if let Some(old_parent) = self.nodes[child].parent {
            self.detach_child(old_parent, child)?;
        }

        {
            let node = self
                .nodes
                .get_mut(child)
                .ok_or_else(|| Error::Internal("Missing child node".into()))?;
            node.parent = Some(parent);
        }

        {
            let node = self
                .nodes
                .get_mut(parent)
                .ok_or_else(|| Error::Internal("Missing parent node".into()))?;
            node.children.push(child);
        }

        let parent_taffy = self.nodes[parent].taffy_id;
        let child_taffy = self.nodes[child].taffy_id;
        self.taffy
            .add_child(parent_taffy, child_taffy)
            .map_err(|err| map_taffy_error(&err))?;

        Ok(())
    }

    /// Remove a child from its parent in both the arena and Taffy.
    pub fn detach_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        {
            let node = self
                .nodes
                .get_mut(parent)
                .ok_or_else(|| Error::Internal("Missing parent node".into()))?;
            node.children.retain(|id| *id != child);
        }

        {
            let node = self
                .nodes
                .get_mut(child)
                .ok_or_else(|| Error::Internal("Missing child node".into()))?;
            node.parent = None;
        }

        let parent_taffy = self.nodes[parent].taffy_id;
        let child_taffy = self.nodes[child].taffy_id;
        self.taffy
            .remove_child(parent_taffy, child_taffy)
            .map_err(|err| map_taffy_error(&err))?;

        Ok(())
    }

    /// Replace the children list for a parent in both the arena and Taffy.
    pub fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        let old_children = self.nodes[parent].children.clone();
        for child in old_children {
            if let Some(node) = self.nodes.get_mut(child) {
                node.parent = None;
            }
        }

        for child in &children {
            if let Some(node) = self.nodes.get_mut(*child) {
                node.parent = Some(parent);
            }
        }

        self.nodes[parent].children = children;

        let parent_taffy = self.nodes[parent].taffy_id;
        let taffy_children: Vec<TaffyNode> = self.nodes[parent]
            .children
            .iter()
            .filter_map(|id| self.nodes.get(*id).map(|n| n.taffy_id))
            .collect();
        self.taffy
            .set_children(parent_taffy, &taffy_children)
            .map_err(|err| map_taffy_error(&err))?;

        Ok(())
    }

    /// Set a node's hidden flag. Returns `true` if visibility changed.
    pub fn set_hidden(&mut self, node_id: NodeId, hidden: bool) -> bool {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return false;
        };
        let changed = node.hidden != hidden;
        node.hidden = hidden;
        if changed {
            self.ensure_focus_visible();
        }
        changed
    }

    /// Hide a node. Returns `true` if visibility changed.
    pub fn hide(&mut self, node_id: NodeId) -> bool {
        self.set_hidden(node_id, true)
    }

    /// Show a node. Returns `true` if visibility changed.
    pub fn show(&mut self, node_id: NodeId) -> bool {
        self.set_hidden(node_id, false)
    }

    /// Start a builder chain for a node.
    pub fn build(&mut self, id: NodeId) -> NodeBuilder<'_> {
        NodeBuilder { core: self, id }
    }

    /// Run layout computation and synchronize viewports.
    pub fn update_layout(&mut self, screen_size: Expanse) -> Result<()> {
        let root_taffy = self.nodes[self.root].taffy_id;
        {
            let mut style = self.nodes[self.root].style.clone();
            style.size.width = Dimension::Points(screen_size.w as f32);
            style.size.height = Dimension::Points(screen_size.h as f32);
            self.taffy
                .set_style(root_taffy, style.clone())
                .map_err(|err| map_taffy_error(&err))?;
            self.nodes[self.root].style = style;
        }
        let available_space = Size {
            width: AvailableSpace::Definite(screen_size.w as f32),
            height: AvailableSpace::Definite(screen_size.h as f32),
        };

        let ctx = MeasureContext { nodes: &self.nodes };
        let ctx_ptr = &ctx as *const _ as *const ();
        MEASURE_CONTEXT.set(&ctx_ptr, || {
            self.taffy
                .compute_layout(root_taffy, available_space)
                .map_err(|err| map_taffy_error(&err))
        })?;

        let screen_vp = ViewPort::new(screen_size, screen_size.rect(), (0, 0))?;
        let mut view_stack = ViewStack::new(screen_vp);
        let root = self.root;
        sync_viewports(&mut self.nodes, &self.taffy, root, &mut view_stack)?;

        self.ensure_focus_visible();

        Ok(())
    }

    /// Take a mutable reference to a widget for a single call.
    pub(crate) fn with_widget_mut<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &mut Self) -> R,
    ) -> R {
        let widget = self.nodes[node_id]
            .widget
            .take()
            .expect("Widget missing from node");
        let mut widget = widget;
        let result = f(&mut *widget, self);
        self.nodes[node_id].widget = Some(widget);
        result
    }

    /// Take a mutable reference to a widget if present, returning None if unavailable.
    pub(crate) fn with_widget_mut_opt<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &mut Self) -> R,
    ) -> Option<R> {
        let widget = {
            let node = self.nodes.get_mut(node_id)?;
            node.widget.take()?
        };
        let mut widget = widget;
        let result = f(&mut *widget, self);
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.widget = Some(widget);
        }
        Some(result)
    }

    /// Take a mutable reference to a widget for rendering with a shared Core context.
    pub(crate) fn with_widget_view<R>(
        &mut self,
        node_id: NodeId,
        f: impl FnOnce(&mut dyn Widget, &Self) -> R,
    ) -> R {
        let widget = self.nodes[node_id]
            .widget
            .take()
            .expect("Widget missing from node");
        let mut widget = widget;
        let core_ptr: *const Self = self;
        // SAFETY: we only provide shared access during the closure and do not mutate Core there.
        let result = f(&mut *widget, unsafe { &*core_ptr });
        self.nodes[node_id].widget = Some(widget);
        result
    }

    /// Check whether a node is on the focus path.
    pub fn is_on_focus_path(&self, node: NodeId) -> bool {
        let mut current = self.focus;
        while let Some(id) = current {
            if id == node {
                return true;
            }
            current = self.nodes[id].parent;
        }
        false
    }

    /// Does the node have terminal focus?
    pub fn is_focused(&self, node: NodeId) -> bool {
        self.focus == Some(node)
    }

    /// Focus a node. Returns `true` if focus changed.
    pub fn set_focus(&mut self, node: NodeId) -> bool {
        if self.is_focused(node) {
            false
        } else {
            self.focus_gen = self.focus_gen.saturating_add(1);
            self.focus = Some(node);
            true
        }
    }

    /// Return the focus path for the subtree under `root`.
    pub fn focus_path(&self, root: NodeId) -> Path {
        let mut parts = Vec::new();
        let mut current = self.focus;
        while let Some(id) = current {
            parts.push(self.nodes[id].name.to_string());
            if id == root {
                break;
            }
            current = self.nodes[id].parent;
        }
        if current != Some(root) {
            return Path::empty();
        }
        parts.reverse();
        Path::new(parts)
    }

    /// Focus the first node that accepts focus in the pre-order traversal of the subtree at root.
    pub fn focus_first(&mut self, root: NodeId) {
        if let Some(target) = self.first_focusable(root) {
            self.set_focus(target);
        }
    }

    /// Focus the next node in the pre-order traversal of root.
    pub fn focus_next(&mut self, root: NodeId) {
        let mut focus_seen = false;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let (hidden, children, view_zero) = {
                let node = &self.nodes[id];
                (node.hidden, node.children.clone(), node.vp.view().is_zero())
            };
            if hidden {
                continue;
            }
            if focus_seen {
                if self.node_accepts_focus(id) && !view_zero {
                    self.set_focus(id);
                    return;
                }
            } else if self.is_focused(id) {
                focus_seen = true;
            }
            for child in children.iter().rev() {
                stack.push(*child);
            }
        }
        self.focus_first(root);
    }

    /// Focus the previous node in the pre-order traversal of `root`.
    pub fn focus_prev(&mut self, root: NodeId) {
        let mut prev_visible = None;
        let mut prev_any = None;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let (hidden, children, view_zero) = {
                let node = &self.nodes[id];
                (node.hidden, node.children.clone(), node.vp.view().is_zero())
            };
            if hidden {
                continue;
            }
            if self.is_focused(id)
                && let Some(target) = prev_visible
            {
                self.set_focus(target);
                return;
            }
            if self.node_accepts_focus(id) {
                prev_any = Some(id);
                if !view_zero {
                    prev_visible = Some(id);
                }
            }
            for child in children.iter().rev() {
                stack.push(*child);
            }
        }
        if let Some(last) = prev_visible.or(prev_any) {
            self.set_focus(last);
        }
    }

    /// Move focus in a specified direction within the subtree at root.
    pub fn focus_dir(&mut self, root: NodeId, dir: Direction) {
        let mut focusables = Vec::new();
        let mut fallback = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let (hidden, children, view_zero) = {
                let node = &self.nodes[id];
                (node.hidden, node.children.clone(), node.vp.view().is_zero())
            };
            if hidden {
                continue;
            }
            if self.node_accepts_focus(id) {
                if view_zero {
                    fallback.push(id);
                } else {
                    focusables.push(id);
                }
            }
            for child in children.iter().rev() {
                stack.push(*child);
            }
        }

        if focusables.is_empty() {
            focusables = fallback;
        }

        let current = match self.focus {
            Some(id) => id,
            None => {
                if let Some(first) = focusables.first().copied() {
                    self.set_focus(first);
                }
                return;
            }
        };

        let current_rect = match self.nodes.get(current).map(|n| n.viewport) {
            Some(r) => r,
            None => return,
        };

        let mut candidates: Vec<(NodeId, Rect)> = focusables
            .into_iter()
            .filter(|id| *id != current)
            .filter_map(|id| self.nodes.get(id).map(|n| (id, n.viewport)))
            .collect();

        let current_center = Point {
            x: current_rect.tl.x + current_rect.w / 2,
            y: current_rect.tl.y + current_rect.h / 2,
        };

        candidates.retain(|(_, rect)| {
            let center = Point {
                x: rect.tl.x + rect.w / 2,
                y: rect.tl.y + rect.h / 2,
            };
            match dir {
                Direction::Right | Direction::Left => {
                    let center_ok = match dir {
                        Direction::Right => center.x > current_center.x,
                        Direction::Left => center.x < current_center.x,
                        _ => false,
                    };
                    let vertical_overlap = rect.tl.y < current_rect.tl.y + current_rect.h
                        && rect.tl.y + rect.h > current_rect.tl.y;
                    center_ok && vertical_overlap
                }
                Direction::Down | Direction::Up => {
                    let center_ok = match dir {
                        Direction::Down => center.y > current_center.y,
                        Direction::Up => center.y < current_center.y,
                        _ => false,
                    };
                    let horizontal_overlap = rect.tl.x < current_rect.tl.x + current_rect.w
                        && rect.tl.x + rect.w > current_rect.tl.x;
                    center_ok && horizontal_overlap
                }
            }
        });

        if candidates.is_empty() {
            return;
        }

        candidates.sort_by_key(|(_, rect)| match dir {
            Direction::Right => {
                let edge_dist = rect.tl.x.saturating_sub(current_rect.tl.x + current_rect.w);
                let vert_center_dist = current_center.y.abs_diff(rect.tl.y + rect.h / 2);
                (edge_dist as u64) * 10000 + (vert_center_dist as u64)
            }
            Direction::Left => {
                let edge_dist = current_rect.tl.x.saturating_sub(rect.tl.x + rect.w);
                let vert_center_dist = current_center.y.abs_diff(rect.tl.y + rect.h / 2);
                (edge_dist as u64) * 10000 + (vert_center_dist as u64)
            }
            Direction::Down => {
                let edge_dist = rect.tl.y.saturating_sub(current_rect.tl.y + current_rect.h);
                let horiz_center_dist = current_center.x.abs_diff(rect.tl.x + rect.w / 2);
                (edge_dist as u64) * 10000 + (horiz_center_dist as u64)
            }
            Direction::Up => {
                let edge_dist = current_rect.tl.y.saturating_sub(rect.tl.y + rect.h);
                let horiz_center_dist = current_center.x.abs_diff(rect.tl.x + rect.w / 2);
                (edge_dist as u64) * 10000 + (horiz_center_dist as u64)
            }
        });

        if let Some((target, _)) = candidates.first().copied() {
            self.set_focus(target);
        }
    }

    /// Check whether a node reports it can accept focus.
    fn node_accepts_focus(&mut self, node_id: NodeId) -> bool {
        self.with_widget_mut_opt(node_id, |w, _| w.accept_focus())
            .unwrap_or(false)
    }

    /// Find the first focusable node, preferring nodes with non-zero view size.
    fn first_focusable(&mut self, root: NodeId) -> Option<NodeId> {
        let mut fallback = None;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let (hidden, children, view_zero) = match self.nodes.get(id) {
                Some(node) => (node.hidden, node.children.clone(), node.vp.view().is_zero()),
                None => continue,
            };
            if hidden {
                continue;
            }
            if self.node_accepts_focus(id) {
                if !view_zero {
                    return Some(id);
                }
                if fallback.is_none() {
                    fallback = Some(id);
                }
            }
            for child in children.iter().rev() {
                stack.push(*child);
            }
        }
        fallback
    }

    /// Ensure focus is not parked on a hidden or zero-sized node.
    fn ensure_focus_visible(&mut self) {
        let Some(focus) = self.focus else {
            return;
        };

        let focus_visible = self
            .nodes
            .get(focus)
            .is_some_and(|node| !node.hidden && !node.vp.view().is_zero());

        if focus_visible {
            return;
        }

        if let Some(target) = self.first_focusable(self.root) {
            self.set_focus(target);
        }
    }

    /// Dispatch an event to a node, bubbling to parents if unhandled.
    pub fn dispatch_event(&mut self, start: NodeId, event: &Event) -> EventOutcome {
        let mut target = Some(start);
        while let Some(id) = target {
            let outcome = self.with_widget_mut(id, |w, core| {
                let mut ctx = CoreContext::new(core, id);
                w.on_event(event, &mut ctx)
            });
            match outcome {
                EventOutcome::Handle | EventOutcome::Consume => return outcome,
                EventOutcome::Ignore => {
                    target = self.nodes[id].parent;
                }
            }
        }
        EventOutcome::Ignore
    }

    /// Dispatch an event to a single node without bubbling.
    pub fn dispatch_event_on_node(&mut self, node_id: NodeId, event: &Event) -> EventOutcome {
        self.with_widget_mut(node_id, |w, core| {
            let mut ctx = CoreContext::new(core, node_id);
            w.on_event(event, &mut ctx)
        })
    }

    /// Return the path for a node relative to a root.
    pub fn node_path(&self, root: NodeId, node_id: NodeId) -> Path {
        let mut parts = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            if let Some(node) = self.nodes.get(id) {
                parts.push(node.name.to_string());
                if id == root {
                    break;
                }
                current = node.parent;
            } else {
                break;
            }
        }
        if current != Some(root) {
            return Path::empty();
        }
        parts.reverse();
        Path::new(parts)
    }

    /// Return the focus path as node IDs from root to focus.
    pub fn focus_path_ids(&self) -> Vec<NodeId> {
        let mut ids = Vec::new();
        let mut current = self.focus;
        while let Some(id) = current {
            ids.push(id);
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        ids.reverse();
        ids
    }

    /// Locate the deepest node under a screen-space point.
    pub fn locate_node(&self, root: NodeId, point: Point) -> Result<Option<NodeId>> {
        let root_vp = self
            .nodes
            .get(root)
            .ok_or_else(|| Error::Internal("missing root node".into()))?
            .vp;
        let screen_vp = ViewPort::new(root_vp.canvas(), root_vp.canvas().rect(), (0, 0))?;
        let mut view_stack = ViewStack::new(screen_vp);
        let mut result = None;
        locate_recursive(self, root, point, &mut view_stack, &mut result)?;
        Ok(result)
    }
}

/// Synchronize viewports from Taffy layout results.
fn sync_viewports(
    nodes: &mut SlotMap<NodeId, Node>,
    taffy: &Taffy,
    node_id: NodeId,
    view_stack: &mut ViewStack,
) -> Result<()> {
    let layout = taffy
        .layout(nodes[node_id].taffy_id)
        .map_err(|err| map_taffy_error(&err))?;

    let start_x = layout.location.x.round().max(0.0);
    let start_y = layout.location.y.round().max(0.0);
    let end_x = (layout.location.x + layout.size.width).round().max(start_x);
    let end_y = (layout.location.y + layout.size.height)
        .round()
        .max(start_y);
    let mut view_size = Expanse::new((end_x - start_x) as u32, (end_y - start_y) as u32);

    let mut canvas_size = if let Some(widget) = nodes[node_id].widget.as_ref() {
        let available = Size {
            width: AvailableSpace::Definite(view_size.w as f32),
            height: AvailableSpace::Definite(view_size.h as f32),
        };
        let measured = widget.canvas_size(
            Size {
                width: None,
                height: None,
            },
            available,
        );
        Expanse::new(
            measured.width.round() as u32,
            measured.height.round() as u32,
        )
    } else {
        view_size
    };

    let min_size = min_size_from_style(&nodes[node_id].style);
    view_size = clamp_expanse(view_size, min_size);
    canvas_size = clamp_expanse(canvas_size, min_size);

    {
        let parent_canvas = view_stack.top().canvas().rect();
        let node = &mut nodes[node_id];
        node.vp.fit_size(canvas_size, view_size);
        let raw_position = Point {
            x: start_x as u32,
            y: start_y as u32,
        };
        node.vp
            .set_position(clamp_child_position(parent_canvas, view_size, raw_position));
    }

    let mut pushed = false;
    let vp = nodes[node_id].vp;
    if !vp.view().is_zero() {
        view_stack.push(vp);
        pushed = true;
    }

    if let Some((_, screen_rect)) = view_stack.projection() {
        nodes[node_id].viewport = screen_rect;
    } else {
        nodes[node_id].viewport = Rect::zero();
    }

    let children = nodes[node_id].children.clone();
    for child in children {
        sync_viewports(nodes, taffy, child, view_stack)?;
    }

    if pushed {
        view_stack.pop()?;
    }

    Ok(())
}

/// Clamp a child viewport position so it remains within the parent canvas bounds.
fn clamp_child_position(parent_canvas: Rect, view_size: Expanse, position: Point) -> Point {
    let max_x = parent_canvas.tl.x.saturating_add(parent_canvas.w);
    let max_y = parent_canvas.tl.y.saturating_add(parent_canvas.h);
    let max_x = if view_size.w == 0 {
        max_x
    } else {
        max_x.saturating_sub(1)
    };
    let max_y = if view_size.h == 0 {
        max_y
    } else {
        max_y.saturating_sub(1)
    };

    Point {
        x: position.x.clamp(parent_canvas.tl.x, max_x),
        y: position.y.clamp(parent_canvas.tl.y, max_y),
    }
}

/// Clamp a size so it respects a minimum width and height.
fn clamp_expanse(size: Expanse, min_size: Expanse) -> Expanse {
    Expanse::new(size.w.max(min_size.w), size.h.max(min_size.h))
}

/// Derive a minimum layout size from a node's style.
fn min_size_from_style(style: &Style) -> Expanse {
    Expanse::new(
        dimension_to_min(style.min_size.width),
        dimension_to_min(style.min_size.height),
    )
}

/// Convert a min-size dimension into a non-negative cell count.
fn dimension_to_min(dim: Dimension) -> u32 {
    match dim {
        Dimension::Points(points) => points.max(0.0).ceil() as u32,
        _ => 0,
    }
}

/// Walk the subtree to find a node containing a point.
fn locate_recursive(
    core: &Core,
    node_id: NodeId,
    point: Point,
    view_stack: &mut ViewStack,
    result: &mut Option<NodeId>,
) -> Result<()> {
    let node = core
        .nodes
        .get(node_id)
        .ok_or_else(|| Error::Internal("missing node".into()))?;

    if node.hidden {
        return Ok(());
    }

    let mut pushed = false;
    let vp = node.vp;
    if !vp.view().is_zero() {
        view_stack.push(vp);
        pushed = true;
    }

    if pushed
        && let Some((_, screen_rect)) = view_stack.projection()
        && screen_rect.contains_point(point)
    {
        *result = Some(node_id);
        let children = node.children.clone();
        for child in children {
            locate_recursive(core, child, point, view_stack, result)?;
        }
    }

    if pushed {
        view_stack.pop()?;
    }

    Ok(())
}

/// Convert a Taffy error into a Canopy error.
fn map_taffy_error(err: &TaffyError) -> Error {
    Error::Layout(err.to_string())
}

#[derive(Default)]
/// Root widget container used for the implicit root node.
struct RootContainer;

impl Widget for RootContainer {
    fn render(&mut self, _frame: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, _ctx: &mut dyn Context) -> Option<Duration> {
        None
    }
}

impl CommandNode for RootContainer {
    fn commands() -> Vec<CommandSpec>
    where
        Self: Sized,
    {
        Vec::new()
    }

    fn dispatch(&mut self, _c: &mut dyn Context, cmd: &CommandInvocation) -> Result<ReturnValue> {
        Err(Error::UnknownCommand(cmd.command.clone()))
    }
}
