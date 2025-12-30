use std::{cell::Cell, collections::HashMap};

use slotmap::SlotMap;

use crate::{
    Context, ViewContext,
    backend::BackendControl,
    commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
    core::{
        context::{CoreContext, CoreViewContext},
        id::NodeId,
        node::Node,
        view::View,
    },
    error::{Error, Result},
    event::Event,
    geom::{Direction, Expanse, Point, Rect, RectI32},
    layout::{
        Align, CanvasChild, CanvasContext, Constraint, Direction as LayoutDirection, Display,
        Layout, MeasureConstraints, Measurement, Size, Sizing, max_bound,
    },
    path::Path,
    render::Render,
    state::NodeName,
    style::StyleMap,
    widget::{EventOutcome, Widget},
};

/// Core state for the arena, layout engine, and focus.
pub struct Core {
    /// Node storage arena.
    pub(crate) nodes: SlotMap<NodeId, Node>,
    /// Root node ID.
    pub(crate) root: NodeId,
    /// Currently focused node.
    pub(crate) focus: Option<NodeId>,
    /// Focus generation counter.
    pub(crate) focus_gen: u64,
    /// Active backend controller.
    pub(crate) backend: Option<Box<dyn BackendControl>>,
    /// Pending style map to be applied before next render.
    pub(crate) pending_style: Option<StyleMap>,
}

impl Core {
    /// Create a new Core with a default root node.
    pub fn new() -> Self {
        let mut nodes = SlotMap::with_key();
        let root_widget = RootContainer;
        let layout = root_widget.layout();
        let root_name = root_widget.name();
        let root = nodes.insert(Node {
            widget: Some(Box::new(root_widget)),
            parent: None,
            children: Vec::new(),
            layout,
            rect: Rect::zero(),
            content_size: Expanse::default(),
            canvas: Expanse::default(),
            scroll: Point::zero(),
            view: View::default(),
            hidden: false,
            name: root_name,
            initialized: false,
            mounted: false,
            layout_dirty: Cell::new(false),
            effects: None,
            clear_inherited_effects: false,
        });

        Self {
            nodes,
            root,
            focus: None,
            focus_gen: 1,
            backend: None,
            pending_style: None,
        }
    }

    /// Return the root node id.
    pub fn root_id(&self) -> NodeId {
        self.root
    }

    /// Return the currently focused node id, if any.
    pub fn focus_id(&self) -> Option<NodeId> {
        self.focus
    }

    /// Return the focus generation counter.
    pub fn focus_generation(&self) -> u64 {
        self.focus_gen
    }

    /// Return a reference to a node by id.
    pub fn node(&self, node_id: NodeId) -> Option<&Node> {
        self.nodes.get(node_id)
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
        let layout = widget.layout();
        let name = widget.name();

        self.nodes.insert(Node {
            widget: Some(widget),
            parent: None,
            children: Vec::new(),
            layout,
            rect: Rect::zero(),
            content_size: Expanse::default(),
            canvas: Expanse::default(),
            scroll: Point::zero(),
            view: View::default(),
            hidden: false,
            name,
            initialized: false,
            mounted: false,
            layout_dirty: Cell::new(false),
            effects: None,
            clear_inherited_effects: false,
        })
    }

    /// Update the layout for a node.
    pub fn with_layout_of(&mut self, node: NodeId, f: impl FnOnce(&mut Layout)) -> Result<()> {
        let node_ref = self
            .nodes
            .get(node)
            .ok_or_else(|| Error::Internal("missing node".into()))?;
        let mut layout = node_ref.layout;
        f(&mut layout);
        if let Some(node) = self.nodes.get_mut(node) {
            node.layout = layout;
        }
        Ok(())
    }

    /// Replace the widget stored at a node.
    pub fn set_widget<W>(&mut self, node_id: NodeId, widget: W)
    where
        W: Widget + 'static,
    {
        let name = widget.name();
        let layout = widget.layout();
        let node = self.nodes.get_mut(node_id).expect("Unknown node id");
        node.widget = Some(Box::new(widget));
        node.name = name;
        node.layout = layout;
        node.mounted = false;
        node.initialized = false;
    }

    /// Run the mount hook for a node if it has not been mounted yet.
    pub(crate) fn mount_node(&mut self, node_id: NodeId) -> Result<()> {
        let should_mount = self
            .nodes
            .get(node_id)
            .map(|node| !node.mounted)
            .unwrap_or(false);
        if !should_mount {
            return Ok(());
        }

        self.with_widget_mut(node_id, |widget, core| {
            let mut ctx = CoreContext::new(core, node_id);
            widget.on_mount(&mut ctx)
        })?;

        if let Some(node) = self.nodes.get_mut(node_id) {
            node.mounted = true;
        }

        Ok(())
    }

    /// Return true if `ancestor` appears in the parent chain of `node`.
    fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Mount a child under a parent in the arena tree.
    pub fn mount_child(&mut self, parent: NodeId, child: NodeId) -> Result<()> {
        if parent == child || self.is_ancestor(child, parent) {
            return Err(Error::Invalid(format!(
                "cannot mount node {child:?} under {parent:?} due to cycle"
            )));
        }

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

        self.mount_node(child)?;

        Ok(())
    }

    /// Remove a child from its parent in the arena tree.
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

        Ok(())
    }

    /// Replace the children list for a parent in the arena tree.
    pub fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) -> Result<()> {
        for child in &children {
            if *child == parent || self.is_ancestor(*child, parent) {
                return Err(Error::Invalid(format!(
                    "cannot set children on {parent:?} with {child:?} due to cycle"
                )));
            }
            if !self.nodes.contains_key(*child) {
                return Err(Error::Internal(format!("Missing child node {child:?}")));
            }
        }

        for child in &children {
            let old_parent = self.nodes.get(*child).and_then(|n| n.parent);
            if let Some(old_parent) = old_parent
                && old_parent != parent
            {
                self.detach_child(old_parent, *child)?;
            }
        }

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

        let new_children = self.nodes[parent].children.clone();
        for child in new_children {
            self.mount_node(child)?;
        }

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

    /// Run layout computation and synchronize views.
    pub fn update_layout(&mut self, screen_size: Expanse) -> Result<()> {
        refresh_layouts(self);
        let root = self.root;
        let mut pass = LayoutPass::new(self);
        pass.layout_node(root, screen_size, Point::zero(), Overflow::none());
        let screen_view = View::new(
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            Point::zero(),
            screen_size,
        );
        pass.update_views(root, screen_view);

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
                (node.hidden, node.children.clone(), node.view.is_zero())
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
                (node.hidden, node.children.clone(), node.view.is_zero())
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
                (node.hidden, node.children.clone(), node.view.is_zero())
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

        let current_rect = match self.nodes.get(current).map(|n| n.view.outer) {
            Some(r) => r,
            None => return,
        };

        let mut candidates: Vec<(NodeId, RectI32)> = focusables
            .into_iter()
            .filter(|id| *id != current)
            .filter_map(|id| self.nodes.get(id).map(|n| (id, n.view.outer)))
            .collect();

        let current_center = rect_center(current_rect);

        candidates.retain(|(_, rect)| {
            let center = rect_center(*rect);
            match dir {
                Direction::Right | Direction::Left => {
                    let center_ok = match dir {
                        Direction::Right => center.0 > current_center.0,
                        Direction::Left => center.0 < current_center.0,
                        _ => false,
                    };
                    let vertical_overlap = rect_overlap_vertical(*rect, current_rect);
                    center_ok && vertical_overlap
                }
                Direction::Down | Direction::Up => {
                    let center_ok = match dir {
                        Direction::Down => center.1 > current_center.1,
                        Direction::Up => center.1 < current_center.1,
                        _ => false,
                    };
                    let horizontal_overlap = rect_overlap_horizontal(*rect, current_rect);
                    center_ok && horizontal_overlap
                }
            }
        });

        if candidates.is_empty() {
            return;
        }

        candidates.sort_by_key(|(_, rect)| match dir {
            Direction::Right => {
                let edge_dist = (rect_left(*rect) - rect_right(current_rect)).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(rect_center(*rect).1) as u64;
                edge_dist * 10000 + vert_center_dist
            }
            Direction::Left => {
                let edge_dist = (rect_left(current_rect) - rect_right(*rect)).max(0) as u64;
                let vert_center_dist = current_center.1.abs_diff(rect_center(*rect).1) as u64;
                edge_dist * 10000 + vert_center_dist
            }
            Direction::Down => {
                let edge_dist = (rect_top(*rect) - rect_bottom(current_rect)).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(rect_center(*rect).0) as u64;
                edge_dist * 10000 + horiz_center_dist
            }
            Direction::Up => {
                let edge_dist = (rect_top(current_rect) - rect_bottom(*rect)).max(0) as u64;
                let horiz_center_dist = current_center.0.abs_diff(rect_center(*rect).0) as u64;
                edge_dist * 10000 + horiz_center_dist
            }
        });

        if let Some((target, _)) = candidates.first().copied() {
            self.set_focus(target);
        }
    }

    /// Check whether a node reports it can accept focus.
    fn node_accepts_focus(&self, node_id: NodeId) -> bool {
        self.nodes
            .get(node_id)
            .and_then(|node| node.widget.as_ref())
            .is_some_and(|widget| {
                let ctx = CoreViewContext::new(self, node_id);
                widget.accept_focus(&ctx)
            })
    }

    /// Find the first focusable node, preferring nodes with non-zero view size.
    fn first_focusable(&self, root: NodeId) -> Option<NodeId> {
        let mut fallback = None;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let (hidden, children, view_zero) = match self.nodes.get(id) {
                Some(node) => (node.hidden, node.children.clone(), node.view.is_zero()),
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
            .is_some_and(|node| !node.hidden && !node.view.is_zero());

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
        let root_view = self
            .nodes
            .get(root)
            .ok_or_else(|| Error::Internal("missing root node".into()))?
            .view;
        let clip = root_view
            .outer
            .intersect_rect(Rect::new(0, 0, root_view.outer.w, root_view.outer.h))
            .unwrap_or_else(Rect::zero);
        let mut result = None;
        locate_recursive(self, root, point, clip, &mut result)?;
        Ok(result)
    }
}

/// Refresh cached layout configurations for nodes marked dirty.
fn refresh_layouts(core: &mut Core) {
    for (_id, node) in core.nodes.iter_mut() {
        if !node.layout_dirty.get() {
            continue;
        }
        if let Some(widget) = node.widget.as_ref() {
            node.layout = widget.layout();
        }
        node.layout_dirty.set(false);
    }
}

#[derive(Hash, PartialEq, Eq)]
/// Cache key for per-pass measurements.
struct MeasureKey {
    /// Node being measured.
    node: NodeId,
    /// Constraints used for the measurement.
    constraints: MeasureConstraints,
}

/// Layout traversal with per-pass measurement caching.
struct LayoutPass<'a> {
    /// Core state being updated.
    core: &'a mut Core,
    /// Cached measurements for this pass.
    measure_cache: HashMap<MeasureKey, Measurement>,
}

#[derive(Clone, Copy)]
/// Overflow flags propagated from parent layouts.
struct Overflow {
    /// Allow horizontal overflow during measurement.
    x: bool,
    /// Allow vertical overflow during measurement.
    y: bool,
}

impl Overflow {
    /// Return a zero-overflow configuration.
    fn none() -> Self {
        Self { x: false, y: false }
    }

    /// Build overflow flags from a layout.
    fn from_layout(layout: Layout) -> Self {
        Self {
            x: layout.overflow_x,
            y: layout.overflow_y,
        }
    }
}

impl<'a> LayoutPass<'a> {
    /// Create a new layout pass with a fresh measurement cache.
    fn new(core: &'a mut Core) -> Self {
        Self {
            core,
            measure_cache: HashMap::new(),
        }
    }

    /// Lay out a node subtree and return its outer size.
    fn layout_node(
        &mut self,
        node_id: NodeId,
        available_outer: Expanse,
        position: Point,
        parent_overflow: Overflow,
    ) -> Size<u32> {
        let (layout, hidden) = self.node_layout_snapshot(node_id);
        if hidden || layout.display == Display::None {
            self.clear_layout(node_id, position);
            return Size::ZERO;
        }

        let mut effective_layout = layout;
        if parent_overflow.x {
            effective_layout.overflow_x = true;
        }
        if parent_overflow.y {
            effective_layout.overflow_y = true;
        }

        let outer = self.resolve_outer_size(node_id, effective_layout, available_outer);
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let content_size = Size::new(
            outer.width.saturating_sub(pad_x),
            outer.height.saturating_sub(pad_y),
        );

        {
            let node = self.core.nodes.get_mut(node_id).expect("missing node");
            node.rect = Rect::new(position.x, position.y, outer.width, outer.height);
            node.content_size = content_size.into();
        }

        self.layout_children(node_id, effective_layout, content_size);

        let canvas = self.compute_canvas(node_id, content_size);
        self.update_canvas(node_id, content_size, canvas);

        outer
    }

    /// Update view rectangles for a subtree based on parent view data.
    fn update_views(&mut self, node_id: NodeId, parent_view: View) {
        let (layout, hidden, rect, content_size, canvas, scroll, children) = {
            let node = match self.core.nodes.get(node_id) {
                Some(node) => node,
                None => return,
            };
            (
                node.layout,
                node.hidden,
                node.rect,
                node.content_size,
                node.canvas,
                node.scroll,
                node.children.clone(),
            )
        };

        if hidden || layout.display == Display::None {
            if let Some(node) = self.core.nodes.get_mut(node_id) {
                node.view = View::default();
            }
            return;
        }

        let outer_x = parent_view.content.tl.x as i64 + rect.tl.x as i64 - parent_view.tl.x as i64;
        let outer_y = parent_view.content.tl.y as i64 + rect.tl.y as i64 - parent_view.tl.y as i64;

        let outer = RectI32::new(
            outer_x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            outer_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            rect.w,
            rect.h,
        );

        let content_x = outer.tl.x as i64 + layout.padding.left as i64;
        let content_y = outer.tl.y as i64 + layout.padding.top as i64;
        let content = RectI32::new(
            content_x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            content_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            content_size.w,
            content_size.h,
        );

        let view = View::new(outer, content, scroll, canvas);
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.view = view;
        }

        for child in children {
            self.update_views(child, view);
        }
    }

    /// Resolve a node's outer size using its layout configuration.
    fn resolve_outer_size(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        available_outer: Expanse,
    ) -> Size<u32> {
        self.resolve_outer_size_with_layout(node_id, layout, available_outer)
    }

    /// Resolve a node's outer size using an explicit layout snapshot.
    fn resolve_outer_size_with_layout(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        available_outer: Expanse,
    ) -> Size<u32> {
        let available: Size<u32> = available_outer.into();
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let available_content_w = available.width.saturating_sub(pad_x);
        let available_content_h = available.height.saturating_sub(pad_y);

        let c0 = MeasureConstraints {
            width: constraint_for_axis(
                layout.width,
                available_content_w,
                layout.min_width,
                layout.max_width,
                pad_x,
                layout.overflow_x,
            ),
            height: constraint_for_axis(
                layout.height,
                available_content_h,
                layout.min_height,
                layout.max_height,
                pad_y,
                layout.overflow_y,
            ),
        };

        let did_measure =
            matches!(layout.width, Sizing::Measure) || matches!(layout.height, Sizing::Measure);

        let mut measured_content = Size::ZERO;
        if did_measure {
            let m0 = self.measure_cached(node_id, c0);
            let raw0 = match m0 {
                Measurement::Fixed(content) => content,
                Measurement::Wrap => self.measure_wrap_content(node_id, layout, c0),
            };
            measured_content = c0.clamp_size(raw0);
        }

        let outer_w0 = match layout.width {
            Sizing::Flex(_) => available.width,
            Sizing::Measure => measured_content.width.saturating_add(pad_x),
        };
        let outer_h0 = match layout.height {
            Sizing::Flex(_) => available.height,
            Sizing::Measure => measured_content.height.saturating_add(pad_y),
        };

        let mut outer = Size::new(outer_w0, outer_h0);
        outer = clamp_outer(outer, layout);

        let mut content = Size::new(
            outer.width.saturating_sub(pad_x),
            outer.height.saturating_sub(pad_y),
        );

        if did_measure {
            let width_seen = match c0.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(_) | Constraint::Unbounded => measured_content.width,
            };

            if content.width != width_seen {
                let c1 = MeasureConstraints {
                    width: Constraint::Exact(content.width),
                    height: c0.height,
                };
                let m1 = self.measure_cached(node_id, c1);
                let raw1 = match m1 {
                    Measurement::Fixed(content) => content,
                    Measurement::Wrap => self.measure_wrap_content(node_id, layout, c1),
                };
                let content1 = c1.clamp_size(raw1);

                if matches!(layout.height, Sizing::Measure) {
                    let outer_h1 = content1.height.saturating_add(pad_y);
                    outer.height = outer_h1;
                    outer = clamp_outer(outer, layout);
                    content = Size::new(
                        outer.width.saturating_sub(pad_x),
                        outer.height.saturating_sub(pad_y),
                    );
                }
            }

            let c_final = MeasureConstraints {
                width: Constraint::Exact(content.width),
                height: Constraint::Exact(content.height),
            };
            let _ = self.measure_cached(node_id, c_final);
        }

        outer
    }

    /// Measure content size by wrapping children when requested.
    fn measure_wrap_content(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        constraints: MeasureConstraints,
    ) -> Size<u32> {
        let children = self.visible_children(node_id);
        if children.is_empty() {
            return Size::ZERO;
        }

        // For Stack direction, content size is the max of all children
        if layout.direction == LayoutDirection::Stack {
            return self.measure_wrap_content_stack(layout, constraints, &children);
        }

        let main_fixed = constraints.main_is_exact(layout.direction);
        let cross_fixed = constraints.cross_is_exact(layout.direction);
        let avail_main = max_bound(constraints.main(layout.direction));
        let avail_cross = max_bound(constraints.cross(layout.direction));
        let avail = Size::from_main_cross(layout.direction, avail_main, avail_cross);

        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut child_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;

            let child_main = main_sizing(child_layout, layout.direction);
            if !main_fixed && matches!(child_main, Sizing::Flex(_)) {
                set_main_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            let child_cross = cross_sizing(child_layout, layout.direction);
            if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            if layout.overflow_x {
                effective.overflow_x = true;
            }
            if layout.overflow_y {
                effective.overflow_y = true;
            }

            let eff_main = main_sizing(effective, layout.direction);
            if let Sizing::Flex(w) = eff_main {
                flex_children.push((i, w.max(1)));
                continue;
            }

            let size = self.resolve_outer_size_with_layout(*child, effective, avail.into());
            child_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(size.main(layout.direction));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = avail_main.saturating_sub(fixed_main_total.saturating_add(gap_total));

        if main_fixed && !flex_children.is_empty() {
            let weights: Vec<u32> = flex_children.iter().map(|(_, w)| (*w).max(1)).collect();
            let shares = allocate_flex_shares(remaining, &weights);
            for (idx, (child_index, _)) in flex_children.iter().enumerate() {
                let child_layout = self.node_layout_snapshot(children[*child_index]).0;
                let mut effective = child_layout;
                let child_cross = cross_sizing(child_layout, layout.direction);
                if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                    set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
                }
                if layout.overflow_x {
                    effective.overflow_x = true;
                }
                if layout.overflow_y {
                    effective.overflow_y = true;
                }
                let child_available =
                    Size::from_main_cross(layout.direction, shares[idx], avail_cross);
                let size = self.resolve_outer_size_with_layout(
                    children[*child_index],
                    effective,
                    child_available.into(),
                );
                child_sizes[*child_index] = size;
            }
        }

        let mut main_total = 0u32;
        let mut cross_max = 0u32;
        for size in &child_sizes {
            main_total = main_total.saturating_add(size.main(layout.direction));
            cross_max = cross_max.max(size.cross(layout.direction));
        }
        main_total = main_total.saturating_add(gap_total);

        let content = Size::from_main_cross(layout.direction, main_total, cross_max);
        constraints.clamp_size(content)
    }

    /// Measure content size for Stack direction - max of all children sizes.
    fn measure_wrap_content_stack(
        &mut self,
        layout: Layout,
        constraints: MeasureConstraints,
        children: &[NodeId],
    ) -> Size<u32> {
        let avail_w = max_bound(constraints.width);
        let avail_h = max_bound(constraints.height);
        let avail = Size::new(avail_w, avail_h);

        let mut max_w = 0u32;
        let mut max_h = 0u32;

        for child in children {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;

            // Treat flex as measure when parent is not exact
            if !matches!(constraints.width, Constraint::Exact(_))
                && matches!(child_layout.width, Sizing::Flex(_))
            {
                effective.width = Sizing::Measure;
            }
            if !matches!(constraints.height, Constraint::Exact(_))
                && matches!(child_layout.height, Sizing::Flex(_))
            {
                effective.height = Sizing::Measure;
            }

            if layout.overflow_x {
                effective.overflow_x = true;
            }
            if layout.overflow_y {
                effective.overflow_y = true;
            }

            let size = self.resolve_outer_size_with_layout(*child, effective, avail.into());
            max_w = max_w.max(size.width);
            max_h = max_h.max(size.height);
        }

        let content = Size::new(max_w, max_h);
        constraints.clamp_size(content)
    }

    /// Lay out visible children inside the provided content box.
    fn layout_children(&mut self, node_id: NodeId, layout: Layout, content: Size<u32>) {
        let children = self.visible_children(node_id);
        if children.is_empty() {
            return;
        }

        let parent_overflow = Overflow::from_layout(layout);
        match layout.direction {
            LayoutDirection::Stack => {
                // Stack: all children get full content area, positioned according to alignment
                for child in &children {
                    // First, layout the child to determine its size
                    self.layout_node(*child, content.into(), Point::zero(), parent_overflow);

                    // Then apply alignment to position the child within content area
                    let child_size = self.node_size(*child);
                    let offset_x =
                        align_offset(child_size.width, content.width, layout.align_horizontal);
                    let offset_y =
                        align_offset(child_size.height, content.height, layout.align_vertical);
                    self.set_node_position(
                        *child,
                        Point {
                            x: offset_x,
                            y: offset_y,
                        },
                    );
                }
            }
            LayoutDirection::Row | LayoutDirection::Column => {
                self.layout_children_sequential(layout, content, &children, parent_overflow);
            }
        }
    }

    /// Layout children sequentially (Row or Column direction).
    fn layout_children_sequential(
        &mut self,
        layout: Layout,
        content: Size<u32>,
        children: &[NodeId],
        parent_overflow: Overflow,
    ) {
        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut pre_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let main = main_sizing(child_layout, layout.direction);
            if let Sizing::Flex(w) = main {
                flex_children.push((i, w.max(1)));
                continue;
            }

            let mut effective = child_layout;
            if parent_overflow.x {
                effective.overflow_x = true;
            }
            if parent_overflow.y {
                effective.overflow_y = true;
            }

            let child_available = content;
            let size =
                self.resolve_outer_size_with_layout(*child, effective, child_available.into());
            pre_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(size.main(layout.direction));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = content
            .main(layout.direction)
            .saturating_sub(fixed_main_total.saturating_add(gap_total));

        let weights: Vec<u32> = flex_children.iter().map(|(_, w)| (*w).max(1)).collect();
        let shares = allocate_flex_shares(remaining, &weights);

        let mut pos_main = 0u32;
        let mut flex_idx = 0usize;
        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child).0;
            let mut effective = child_layout;
            if parent_overflow.x {
                effective.overflow_x = true;
            }
            if parent_overflow.y {
                effective.overflow_y = true;
            }

            let main = match main_sizing(effective, layout.direction) {
                Sizing::Flex(_) => {
                    let share = shares[flex_idx];
                    flex_idx += 1;
                    share
                }
                Sizing::Measure => pre_sizes[i].main(layout.direction),
            };

            let child_available =
                Size::from_main_cross(layout.direction, main, content.cross(layout.direction));
            let child_pos = match layout.direction {
                LayoutDirection::Row => Point { x: pos_main, y: 0 },
                LayoutDirection::Column => Point { x: 0, y: pos_main },
                LayoutDirection::Stack => unreachable!(),
            };

            let actual =
                self.layout_node(*child, child_available.into(), child_pos, parent_overflow);
            pos_main = pos_main
                .saturating_add(actual.main(layout.direction))
                .saturating_add(layout.gap);
        }
    }

    /// Get a node's outer size.
    fn node_size(&self, node_id: NodeId) -> Size<u32> {
        self.core
            .nodes
            .get(node_id)
            .map(|n| Size::new(n.rect.w, n.rect.h))
            .unwrap_or(Size::ZERO)
    }

    /// Set a node's position within its parent's content area.
    fn set_node_position(&mut self, node_id: NodeId, position: Point) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.rect.tl = position;
        }
    }

    /// Compute the scrollable canvas size for a node.
    fn compute_canvas(&mut self, node_id: NodeId, view_size: Size<u32>) -> Size<u32> {
        let children = self.visible_children(node_id);
        let mut canvas_children = Vec::with_capacity(children.len());
        for child in children {
            if let Some(node) = self.core.nodes.get(child) {
                let child_canvas: Size<u32> = node.canvas.into();
                canvas_children.push(CanvasChild::new(node.rect, child_canvas));
            }
        }
        let ctx = CanvasContext::new(&canvas_children);
        let canvas = self
            .core
            .with_widget_view(node_id, |widget, _core| widget.canvas(view_size, &ctx));
        Size::new(
            canvas.width.max(view_size.width),
            canvas.height.max(view_size.height),
        )
    }

    /// Store canvas size and clamp scroll offset for a node.
    fn update_canvas(&mut self, node_id: NodeId, view_size: Size<u32>, canvas: Size<u32>) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            let mut canvas = canvas;
            canvas.width = canvas.width.max(view_size.width);
            canvas.height = canvas.height.max(view_size.height);

            let mut scroll = node.scroll;
            clamp_scroll(&mut scroll, view_size, canvas);
            node.scroll = scroll;
            node.canvas = canvas.into();
        }
    }

    /// Snapshot a node's layout and hidden state.
    fn node_layout_snapshot(&self, node_id: NodeId) -> (Layout, bool) {
        self.core
            .nodes
            .get(node_id)
            .map(|node| (node.layout, node.hidden))
            .unwrap_or((Layout::default(), true))
    }

    /// Collect visible child nodes in tree order.
    fn visible_children(&self, node_id: NodeId) -> Vec<NodeId> {
        let Some(node) = self.core.nodes.get(node_id) else {
            return Vec::new();
        };
        node.children
            .iter()
            .copied()
            .filter(|child| {
                self.core
                    .nodes
                    .get(*child)
                    .is_some_and(|n| !n.hidden && n.layout.display == Display::Block)
            })
            .collect()
    }

    /// Get a cached measurement or compute and store it for this pass.
    fn measure_cached(&mut self, node_id: NodeId, constraints: MeasureConstraints) -> Measurement {
        let key = MeasureKey {
            node: node_id,
            constraints,
        };
        if let Some(m) = self.measure_cache.get(&key) {
            return *m;
        }
        let measured = self
            .core
            .with_widget_view(node_id, |widget, _core| widget.measure(constraints));
        self.measure_cache.insert(key, measured);
        measured
    }

    /// Reset layout data for a hidden subtree.
    fn clear_layout(&mut self, node_id: NodeId, position: Point) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.rect = Rect::new(position.x, position.y, 0, 0);
            node.content_size = Expanse::default();
            node.canvas = Expanse::default();
            node.scroll = Point::zero();
            node.view = View::default();
        }
        let children = self
            .core
            .nodes
            .get(node_id)
            .map(|node| node.children.clone())
            .unwrap_or_default();
        for child in children {
            self.clear_layout(child, Point::zero());
        }
    }
}

/// Clamp an outer size against min/max constraints.
fn clamp_outer(size: Size<u32>, layout: Layout) -> Size<u32> {
    Size::new(
        clamp_axis(size.width, layout.min_width, layout.max_width),
        clamp_axis(size.height, layout.min_height, layout.max_height),
    )
}

/// Clamp a single axis against optional min/max bounds.
fn clamp_axis(value: u32, min: Option<u32>, max: Option<u32>) -> u32 {
    let (min, max) = match (min, max) {
        (Some(min), Some(max)) if min > max => (Some(max), Some(max)),
        other => other,
    };
    let mut value = value;
    if let Some(max) = max {
        value = value.min(max);
    }
    if let Some(min) = min {
        value = value.max(min);
    }
    value
}

/// Build a content-box constraint for a single axis.
fn constraint_for_axis(
    sizing: Sizing,
    available_content: u32,
    min_outer: Option<u32>,
    max_outer: Option<u32>,
    pad_axis: u32,
    overflow: bool,
) -> Constraint {
    match sizing {
        Sizing::Flex(_) => Constraint::Exact(available_content),
        Sizing::Measure => {
            if overflow && max_outer.is_none() {
                return Constraint::Unbounded;
            }
            let effective_max_outer = match max_outer {
                Some(m) => m.min(available_content.saturating_add(pad_axis)),
                None => available_content.saturating_add(pad_axis),
            };
            let effective_max_content = effective_max_outer.saturating_sub(pad_axis);

            if let (Some(min_o), Some(max_o)) = (min_outer, max_outer)
                && min_o == max_o
            {
                return Constraint::Exact(max_o.saturating_sub(pad_axis));
            }

            Constraint::AtMost(effective_max_content)
        }
    }
}

/// Clamp a scroll offset so it stays within view/canvas bounds.
fn clamp_scroll(scroll: &mut Point, view: Size<u32>, canvas: Size<u32>) {
    let max_x = if view.width == 0 {
        0
    } else {
        canvas.width.saturating_sub(view.width)
    };
    let max_y = if view.height == 0 {
        0
    } else {
        canvas.height.saturating_sub(view.height)
    };
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);
}

/// Allocate remaining space proportionally across flex weights.
fn allocate_flex_shares(remaining: u32, weights: &[u32]) -> Vec<u32> {
    if remaining == 0 || weights.is_empty() {
        return vec![0; weights.len()];
    }
    let total: u64 = weights.iter().map(|w| (*w).max(1) as u64).sum();
    if total == 0 {
        return vec![0; weights.len()];
    }

    let mut base = Vec::with_capacity(weights.len());
    let mut rem = Vec::with_capacity(weights.len());
    for w in weights {
        let weight = (*w).max(1) as u64;
        let prod = remaining as u64 * weight;
        base.push((prod / total) as u32);
        rem.push((prod % total) as u32);
    }

    let used: u32 = base.iter().sum();
    let extra = remaining.saturating_sub(used);
    if extra == 0 {
        return base;
    }

    let mut idx: Vec<usize> = (0..weights.len()).collect();
    idx.sort_by(|a, b| rem[*b].cmp(&rem[*a]).then_with(|| a.cmp(b)));
    for i in 0..extra as usize {
        if let Some(target) = idx.get(i) {
            base[*target] = base[*target].saturating_add(1);
        }
    }

    base
}

/// Extract the main-axis sizing from a layout.
fn main_sizing(layout: Layout, direction: LayoutDirection) -> Sizing {
    match direction {
        LayoutDirection::Row => layout.width,
        LayoutDirection::Column | LayoutDirection::Stack => layout.height,
    }
}

/// Extract the cross-axis sizing from a layout.
fn cross_sizing(layout: Layout, direction: LayoutDirection) -> Sizing {
    match direction {
        LayoutDirection::Row => layout.height,
        LayoutDirection::Column | LayoutDirection::Stack => layout.width,
    }
}

/// Set the main-axis sizing on a layout.
fn set_main_sizing(layout: &mut Layout, direction: LayoutDirection, sizing: Sizing) {
    match direction {
        LayoutDirection::Row => layout.width = sizing,
        LayoutDirection::Column | LayoutDirection::Stack => layout.height = sizing,
    }
}

/// Set the cross-axis sizing on a layout.
fn set_cross_sizing(layout: &mut Layout, direction: LayoutDirection, sizing: Sizing) {
    match direction {
        LayoutDirection::Row => layout.height = sizing,
        LayoutDirection::Column | LayoutDirection::Stack => layout.width = sizing,
    }
}

/// Calculate the offset for aligning a child within available space.
fn align_offset(child_size: u32, available: u32, align: Align) -> u32 {
    match align {
        Align::Start => 0,
        Align::Center => available.saturating_sub(child_size) / 2,
        Align::End => available.saturating_sub(child_size),
    }
}

/// Depth-first search for a node at a screen-space point.
fn locate_recursive(
    core: &Core,
    node_id: NodeId,
    point: Point,
    parent_clip: Rect,
    result: &mut Option<NodeId>,
) -> Result<()> {
    let node = core
        .nodes
        .get(node_id)
        .ok_or_else(|| Error::Internal("missing node".into()))?;

    if node.hidden || node.layout.display == Display::None {
        return Ok(());
    }

    let Some(outer_clip) = node.view.outer.intersect_rect(parent_clip) else {
        return Ok(());
    };
    if !outer_clip.contains_point(point) {
        return Ok(());
    }

    *result = Some(node_id);

    let Some(child_clip) = node.view.content.intersect_rect(parent_clip) else {
        return Ok(());
    };
    let children = node.children.clone();
    for child in children {
        locate_recursive(core, child, point, child_clip, result)?;
    }

    Ok(())
}

/// Left edge of a signed rect.
fn rect_left(rect: RectI32) -> i64 {
    rect.tl.x as i64
}

/// Top edge of a signed rect.
fn rect_top(rect: RectI32) -> i64 {
    rect.tl.y as i64
}

/// Right edge of a signed rect.
fn rect_right(rect: RectI32) -> i64 {
    rect.tl.x as i64 + rect.w as i64
}

/// Bottom edge of a signed rect.
fn rect_bottom(rect: RectI32) -> i64 {
    rect.tl.y as i64 + rect.h as i64
}

/// Center point of a signed rect.
fn rect_center(rect: RectI32) -> (i64, i64) {
    (
        rect_left(rect) + rect.w as i64 / 2,
        rect_top(rect) + rect.h as i64 / 2,
    )
}

/// Return true if two rects overlap vertically.
fn rect_overlap_vertical(a: RectI32, b: RectI32) -> bool {
    rect_top(a) < rect_bottom(b) && rect_bottom(a) > rect_top(b)
}

/// Return true if two rects overlap horizontally.
fn rect_overlap_horizontal(a: RectI32, b: RectI32) -> bool {
    rect_left(a) < rect_right(b) && rect_right(a) > rect_left(b)
}

#[derive(Default)]
/// Root widget container used for the implicit root node.
struct RootContainer;

impl Widget for RootContainer {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn render(&mut self, _frame: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn name(&self) -> NodeName {
        NodeName::convert("root")
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rand::{Rng, SeedableRng, rngs::StdRng};

    use super::*;
    use crate::{
        commands::{CommandInvocation, CommandNode, CommandSpec, ReturnValue},
        error::{Error, Result},
        geom::{Expanse, Point},
        layout::{
            Align, CanvasContext, Constraint, Edges, Layout, MeasureConstraints, Measurement, Size,
            Sizing,
        },
        widget::Widget,
    };

    type MeasureFn = dyn Fn(MeasureConstraints) -> Measurement + Send + Sync;
    type CanvasFn = dyn Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync;

    struct TestWidget {
        measure_fn: Arc<MeasureFn>,
        canvas_fn: Arc<CanvasFn>,
    }

    impl TestWidget {
        fn new<F>(measure_fn: F) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
        where
            F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
        {
            Self::with_canvas(measure_fn, |view, _ctx| view)
        }

        fn with_canvas<F, C>(
            measure_fn: F,
            canvas_fn: C,
        ) -> (Self, Arc<Mutex<Vec<MeasureConstraints>>>)
        where
            F: Fn(MeasureConstraints) -> Measurement + Send + Sync + 'static,
            C: Fn(Size<u32>, &CanvasContext) -> Size<u32> + Send + Sync + 'static,
        {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let calls_clone = Arc::clone(&calls);
            let measure_fn = Arc::new(move |c: MeasureConstraints| {
                calls_clone.lock().unwrap().push(c);
                measure_fn(c)
            });
            let canvas_fn = Arc::new(canvas_fn);
            (
                Self {
                    measure_fn,
                    canvas_fn,
                },
                calls,
            )
        }
    }

    impl CommandNode for TestWidget {
        fn commands() -> Vec<CommandSpec>
        where
            Self: Sized,
        {
            Vec::new()
        }

        fn dispatch(
            &mut self,
            _c: &mut dyn Context,
            _cmd: &CommandInvocation,
        ) -> Result<ReturnValue> {
            Ok(ReturnValue::Void)
        }
    }

    impl Widget for TestWidget {
        fn measure(&self, c: MeasureConstraints) -> Measurement {
            (self.measure_fn)(c)
        }

        fn canvas(&self, view: Size<u32>, ctx: &CanvasContext) -> Size<u32> {
            (self.canvas_fn)(view, ctx)
        }
    }

    fn attach_root_child(core: &mut Core, child: NodeId) -> Result<()> {
        core.set_children(core.root, vec![child])
    }

    #[test]
    fn clamp_outer_no_bounds() {
        let layout = Layout::column();
        let size = Size::new(5, 7);
        assert_eq!(clamp_outer(size, layout), size);
    }

    #[test]
    fn clamp_outer_min_only() {
        let mut layout = Layout::column();
        layout.min_width = Some(10);
        layout.min_height = Some(2);
        assert_eq!(clamp_outer(Size::new(5, 1), layout), Size::new(10, 2));
    }

    #[test]
    fn clamp_outer_max_only() {
        let mut layout = Layout::column();
        layout.max_width = Some(3);
        layout.max_height = Some(4);
        assert_eq!(clamp_outer(Size::new(5, 7), layout), Size::new(3, 4));
    }

    #[test]
    fn clamp_outer_min_greater_than_max() {
        let mut layout = Layout::column();
        layout.min_width = Some(10);
        layout.max_width = Some(5);
        assert_eq!(clamp_outer(Size::new(8, 1), layout), Size::new(5, 1));
    }

    #[test]
    fn constraint_for_axis_flex_is_exact() {
        let c = constraint_for_axis(Sizing::Flex(1), 10, None, None, 0, false);
        assert_eq!(c, Constraint::Exact(10));
    }

    #[test]
    fn constraint_for_axis_min_equals_max_is_exact() {
        let c = constraint_for_axis(Sizing::Measure, 10, Some(6), Some(6), 2, false);
        assert_eq!(c, Constraint::Exact(4));
    }

    #[test]
    fn constraint_for_axis_max_caps_available() {
        let c = constraint_for_axis(Sizing::Measure, 10, None, Some(6), 2, false);
        assert_eq!(c, Constraint::AtMost(4));
    }

    #[test]
    fn leaf_measure_adds_padding() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[child];
        assert_eq!(node.rect.w, 7);
        assert_eq!(node.rect.h, 7);
        assert_eq!(node.content_size, Expanse::new(5, 5));
        Ok(())
    }

    #[test]
    fn leaf_padding_consumes_all() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(1, 1))?;
        let node = &core.nodes[child];
        assert_eq!(node.content_size, Expanse::new(0, 0));
        Ok(())
    }

    #[test]
    fn flex_axis_constraints_are_exact() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().flex_horizontal(1);
        })?;
        core.update_layout(Expanse::new(10, 5))?;
        let calls = calls.lock().unwrap();
        assert!(!calls.is_empty());
        assert_eq!(calls[0].width, Constraint::Exact(10));
        Ok(())
    }

    #[test]
    fn remeasure_when_min_width_expands_measured() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                _ => 4,
            };
            let height = if width >= 10 { 2 } else { 4 };
            Measurement::Fixed(Size::new(width, height))
        });
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().min_width(10);
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let calls = calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(10)));
        let node = &core.nodes[child];
        assert_eq!(node.content_size.h, 2);
        Ok(())
    }

    #[test]
    fn remeasure_when_min_width_expands_flex() -> Result<()> {
        let mut core = Core::new();
        let (widget, calls) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                _ => 0,
            };
            Measurement::Fixed(Size::new(width, width.max(1)))
        });
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .padding(Edges::all(1))
                .min_width(30);
        })?;
        core.update_layout(Expanse::new(10, 10))?;
        let calls = calls.lock().unwrap();
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(8)));
        assert!(calls.iter().any(|c| c.width == Constraint::Exact(28)));
        Ok(())
    }

    #[test]
    fn wrap_no_children() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(0, 0));
        assert_eq!(node.rect.w, 2);
        assert_eq!(node.rect.h, 2);
        Ok(())
    }

    #[test]
    fn wrap_sum_main_max_cross() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 3)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 2)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().gap(1);
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(4, 8));
        Ok(())
    }

    #[test]
    fn wrap_includes_child_padding() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(3, 1)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column();
        })?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(50, 50))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size, Expanse::new(5, 3));
        Ok(())
    }

    #[test]
    fn wrap_flex_child_treated_as_measure_when_parent_not_exact() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 4)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column();
        })?;
        core.with_layout_of(child, |layout| {
            layout.height = Sizing::Flex(1);
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size.h, 4);
        Ok(())
    }

    #[test]
    fn wrap_flex_child_behaves_as_flex_when_parent_exact() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child1_widget, calls1) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(n) => n,
                Constraint::Unbounded => 0,
            };
            Measurement::Fixed(Size::new(width, width))
        });
        let (child2_widget, calls2) = TestWidget::new(|c| {
            let width = match c.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(n) => n,
                Constraint::Unbounded => 0,
            };
            Measurement::Fixed(Size::new(width, width))
        });
        let child1 = core.add_boxed(Box::new(child1_widget));
        let child2 = core.add_boxed(Box::new(child2_widget));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(1);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(1);
        })?;
        core.update_layout(Expanse::new(10, 10))?;
        let calls1 = calls1.lock().unwrap();
        let calls2 = calls2.lock().unwrap();
        assert!(calls1.iter().any(|c| c.width == Constraint::Exact(5)));
        assert!(calls2.iter().any(|c| c.width == Constraint::Exact(5)));
        let parent_node = &core.nodes[parent];
        assert_eq!(parent_node.content_size.h, 5);
        Ok(())
    }

    #[test]
    fn wrap_gap_counts_only_visible_children() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().gap(2);
        })?;
        core.with_layout_of(child2, |layout| {
            *layout = Layout::column().none();
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let node = &core.nodes[parent];
        assert_eq!(node.content_size.h, 4);
        Ok(())
    }

    #[test]
    fn flex_shares_sum_equals_remaining() {
        let shares = allocate_flex_shares(17, &[1, 2, 3, 4]);
        let sum: u32 = shares.iter().sum();
        assert_eq!(sum, 17);
    }

    #[test]
    fn flex_shares_proportional_sanity() {
        let shares = allocate_flex_shares(5, &[3, 7]);
        assert_eq!(shares, vec![2, 3]);
    }

    #[test]
    fn flex_shares_stable_tie_break() {
        let shares = allocate_flex_shares(2, &[1, 1, 1]);
        assert_eq!(shares, vec![1, 1, 0]);
    }

    #[test]
    fn flex_weight_zero_clamped() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(0);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(0);
        })?;
        core.update_layout(Expanse::new(10, 5))?;
        assert_eq!(core.nodes[child1].rect.w, 5);
        assert_eq!(core.nodes[child2].rect.w, 5);
        Ok(())
    }

    #[test]
    fn positions_monotonic_main() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let (c3, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        let child3 = core.add_boxed(Box::new(c3));
        core.set_children(parent, vec![child1, child2, child3])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1).gap(1);
        })?;
        core.update_layout(Expanse::new(20, 5))?;
        let p1 = core.nodes[child1].rect.tl.x;
        let p2 = core.nodes[child2].rect.tl.x;
        let p3 = core.nodes[child3].rect.tl.x;
        assert!(p1 <= p2 && p2 <= p3);
        Ok(())
    }

    #[test]
    fn no_overlaps_with_min_expansion() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1);
        })?;
        core.with_layout_of(child1, |layout| {
            layout.width = Sizing::Flex(1);
            layout.min_width = Some(10);
        })?;
        core.with_layout_of(child2, |layout| {
            layout.width = Sizing::Flex(1);
            layout.min_width = Some(10);
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        let first = &core.nodes[child1];
        let second = &core.nodes[child2];
        assert_eq!(second.rect.tl.x, first.rect.tl.x + first.rect.w);
        Ok(())
    }

    #[test]
    fn overflow_positions_consistent() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(4, 1)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::row().flex_horizontal(1).gap(1);
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        assert_eq!(core.nodes[child2].rect.tl.x, 5);
        Ok(())
    }

    #[test]
    fn canvas_clamped_at_least_view() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) =
            TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(1, 1));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        core.update_layout(Expanse::new(5, 5))?;
        let node = &core.nodes[child];
        assert_eq!(node.canvas, Expanse::new(5, 5));
        Ok(())
    }

    #[test]
    fn offset_clamped_when_canvas_shrinks() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 15, y: 15 };
        }
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
        *canvas.lock().unwrap() = Size::new(12, 12);
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 2, y: 2 });
        Ok(())
    }

    #[test]
    fn offset_clamped_when_view_grows() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(20, 20)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 15, y: 15 };
        }
        core.update_layout(Expanse::new(5, 5))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 15, y: 15 });
        core.update_layout(Expanse::new(10, 10))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 10, y: 10 });
        Ok(())
    }

    #[test]
    fn zero_view_clamps_scroll() -> Result<()> {
        let mut core = Core::new();
        let canvas = Arc::new(Mutex::new(Size::new(10, 10)));
        let canvas_clone = Arc::clone(&canvas);
        let (widget, _) = TestWidget::with_canvas(
            |_c| Measurement::Wrap,
            move |_view, _ctx| *canvas_clone.lock().unwrap(),
        );
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::fill();
        })?;
        if let Some(node) = core.nodes.get_mut(child) {
            node.scroll = Point { x: 5, y: 5 };
        }
        core.update_layout(Expanse::new(0, 0))?;
        assert_eq!(core.nodes[child].scroll, Point { x: 0, y: 0 });
        Ok(())
    }

    #[test]
    fn child_screen_origin_signed() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) =
            TestWidget::with_canvas(|_c| Measurement::Wrap, |_view, _ctx| Size::new(20, 10));
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(2, 2)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
        })?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().fixed_width(2).fixed_height(2);
        })?;
        if let Some(node) = core.nodes.get_mut(parent) {
            node.scroll = Point { x: 5, y: 0 };
        }
        core.update_layout(Expanse::new(10, 10))?;
        let child_view = core.nodes[child].view;
        assert_eq!(child_view.outer.tl.x, -5);
        Ok(())
    }

    #[test]
    fn content_rect_respects_padding() -> Result<()> {
        let mut core = Core::new();
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child = core.add_boxed(Box::new(widget));
        attach_root_child(&mut core, child)?;
        core.with_layout_of(child, |layout| {
            *layout = Layout::column().padding(Edges::all(1));
        })?;
        core.update_layout(Expanse::new(20, 20))?;
        let view = core.nodes[child].view;
        assert_eq!(view.content.tl.x, view.outer.tl.x + 1);
        assert_eq!(view.content.tl.y, view.outer.tl.y + 1);
        assert_eq!(view.content.w, view.outer.w.saturating_sub(2));
        assert_eq!(view.content.h, view.outer.h.saturating_sub(2));
        Ok(())
    }

    #[test]
    fn random_tree_no_panics() -> Result<()> {
        let mut core = Core::new();
        let mut rng = StdRng::seed_from_u64(0x5eed);
        let root_child = build_random_tree(&mut core, &mut rng, 3)?;
        attach_root_child(&mut core, root_child)?;
        core.update_layout(Expanse::new(40, 20))?;

        for node in core.nodes.values() {
            let expected_w = node.rect.w.saturating_sub(node.layout.padding.horizontal());
            let expected_h = node.rect.h.saturating_sub(node.layout.padding.vertical());
            assert_eq!(node.content_size.w, expected_w);
            assert_eq!(node.content_size.h, expected_h);
            assert!(node.canvas.w >= node.content_size.w);
            assert!(node.canvas.h >= node.content_size.h);
            let max_x = node.canvas.w.saturating_sub(node.content_size.w);
            let max_y = node.canvas.h.saturating_sub(node.content_size.h);
            assert!(node.scroll.x <= max_x);
            assert!(node.scroll.y <= max_y);
        }

        for node in core.nodes.values() {
            // For Stack direction, children can overlap, so skip position ordering check
            if node.layout.direction == LayoutDirection::Stack {
                continue;
            }
            let mut last = 0u32;
            for child in &node.children {
                let child = &core.nodes[*child];
                if child.layout.display == Display::None || child.hidden {
                    continue;
                }
                let pos = match node.layout.direction {
                    LayoutDirection::Row => child.rect.tl.x,
                    LayoutDirection::Column => child.rect.tl.y,
                    LayoutDirection::Stack => continue,
                };
                assert!(pos >= last);
                last = pos;
            }
        }

        Ok(())
    }

    fn build_random_tree(core: &mut Core, rng: &mut StdRng, depth: usize) -> Result<NodeId> {
        let (widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(1, 1)));
        let node = core.add_boxed(Box::new(widget));
        let mut layout = if rng.random_bool(0.5) {
            Layout::row()
        } else {
            Layout::column()
        };
        if rng.random_bool(0.6) {
            layout.width = Sizing::Flex(rng.random_range(0..3));
        }
        if rng.random_bool(0.6) {
            layout.height = Sizing::Flex(rng.random_range(0..3));
        }
        layout.padding = Edges::new(
            rng.random_range(0..3),
            rng.random_range(0..3),
            rng.random_range(0..3),
            rng.random_range(0..3),
        );
        layout.gap = rng.random_range(0..3);
        if rng.random_bool(0.3) {
            layout.min_width = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.max_width = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.min_height = Some(rng.random_range(0..6));
        }
        if rng.random_bool(0.3) {
            layout.max_height = Some(rng.random_range(0..6));
        }
        core.with_layout_of(node, |l| {
            *l = layout;
        })?;

        if depth > 0 {
            let child_count = rng.random_range(0..=3);
            if child_count > 0 {
                let mut children = Vec::new();
                for _ in 0..child_count {
                    children.push(build_random_tree(core, rng, depth - 1)?);
                }
                core.set_children(node, children)?;
            }
        }

        Ok(node)
    }

    #[test]
    fn stack_children_overlap() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(5, 5)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::stack();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Both children should be at the same position (0, 0) by default
        assert_eq!(core.nodes[child1].rect.tl.x, 0);
        assert_eq!(core.nodes[child1].rect.tl.y, 0);
        assert_eq!(core.nodes[child2].rect.tl.x, 0);
        assert_eq!(core.nodes[child2].rect.tl.y, 0);

        // Parent content size should be the max of children
        let parent_node = &core.nodes[parent];
        assert_eq!(parent_node.content_size, Expanse::new(10, 10));
        Ok(())
    }

    #[test]
    fn stack_with_center_alignment() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::stack()
                .flex_horizontal(1)
                .flex_vertical(1)
                .align_center();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Child should be centered in the 50x50 parent
        let child_node = &core.nodes[child];
        assert_eq!(child_node.rect.tl.x, 20); // (50 - 10) / 2
        assert_eq!(child_node.rect.tl.y, 20); // (50 - 10) / 2
        Ok(())
    }

    #[test]
    fn stack_with_end_alignment() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::stack()
                .flex_horizontal(1)
                .flex_vertical(1)
                .align_horizontal(Align::End)
                .align_vertical(Align::End);
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Child should be at the end (bottom-right)
        let child_node = &core.nodes[child];
        assert_eq!(child_node.rect.tl.x, 40); // 50 - 10
        assert_eq!(child_node.rect.tl.y, 40); // 50 - 10
        Ok(())
    }

    #[test]
    fn stack_multiple_children_centered() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (c1, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(20, 20)));
        let (c2, _) = TestWidget::new(|_c| Measurement::Fixed(Size::new(10, 10)));
        let child1 = core.add_boxed(Box::new(c1));
        let child2 = core.add_boxed(Box::new(c2));
        core.set_children(parent, vec![child1, child2])?;
        attach_root_child(&mut core, parent)?;
        core.with_layout_of(parent, |layout| {
            *layout = Layout::stack()
                .flex_horizontal(1)
                .flex_vertical(1)
                .align_center();
        })?;
        core.update_layout(Expanse::new(50, 50))?;

        // Both children should be centered independently
        let c1_node = &core.nodes[child1];
        let c2_node = &core.nodes[child2];
        assert_eq!(c1_node.rect.tl.x, 15); // (50 - 20) / 2
        assert_eq!(c1_node.rect.tl.y, 15);
        assert_eq!(c2_node.rect.tl.x, 20); // (50 - 10) / 2
        assert_eq!(c2_node.rect.tl.y, 20);
        Ok(())
    }

    #[test]
    fn set_children_detaches_from_previous_parent() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent_a = core.add_boxed(Box::new(parent_widget));
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent_b = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));

        core.set_children(parent_a, vec![child])?;
        core.set_children(parent_b, vec![child])?;

        assert!(core.nodes[parent_a].children.is_empty());
        assert_eq!(core.nodes[parent_b].children, vec![child]);
        assert_eq!(core.nodes[child].parent, Some(parent_b));
        Ok(())
    }

    #[test]
    fn set_children_rejects_cycles() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));
        core.set_children(parent, vec![child])?;

        let err = core.set_children(child, vec![parent]).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
        Ok(())
    }

    #[test]
    fn mount_child_rejects_cycles() -> Result<()> {
        let mut core = Core::new();
        let (parent_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let parent = core.add_boxed(Box::new(parent_widget));
        let (child_widget, _) = TestWidget::new(|_c| Measurement::Wrap);
        let child = core.add_boxed(Box::new(child_widget));

        core.mount_child(parent, child)?;
        let err = core.mount_child(child, parent).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
        Ok(())
    }
}
