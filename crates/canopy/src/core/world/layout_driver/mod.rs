use std::collections::HashMap;

use super::*;
use crate::{
    core::view::View,
    geom::{Point, Rect, RectI32, Size},
    layout::{
        Align, CanvasChild, CanvasContext, Constraint, Direction as LayoutDirection, Display,
        Layout, MeasureConstraints, Measurement, Sizing,
    },
};

impl Core {
    /// Run layout computation and synchronize views.
    pub fn update_layout(&mut self, screen_size: Size) -> Result<()> {
        refresh_layouts(self)?;
        let root = self.root;
        let mut pass = LayoutPass::new(self);
        pass.layout_node(root, screen_size, Point::zero(), Overflow::none())?;
        let screen_view = View::new(
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            RectI32::new(0, 0, screen_size.w, screen_size.h),
            Point::zero(),
            screen_size,
        );
        pass.update_views(root, screen_view)?;

        self.ensure_focus_valid(None)?;
        self.validate_invariants()?;

        Ok(())
    }

    /// Locate the deepest node under a screen-space point.
    pub fn locate_node(&self, root: impl Into<NodeId>, point: Point) -> Result<Option<NodeId>> {
        let root = root.into();
        let root_view = self
            .nodes
            .get(root)
            .ok_or_else(|| Error::Internal("missing root node".into()))?
            .view;
        let clip = root_view
            .outer
            .intersect_rect(Rect::new(0, 0, root_view.outer.w, root_view.outer.h))
            .unwrap_or_else(Rect::zero);
        locate_recursive(self, root, point, clip)
    }
}

/// Refresh cached layout configurations for nodes marked dirty.
pub(super) fn refresh_layouts(core: &mut Core) -> Result<()> {
    let dirty = core
        .nodes
        .iter()
        .filter_map(|(node_id, node)| node.layout_dirty.then_some(node_id))
        .collect::<Vec<_>>();
    for node_id in dirty {
        let layout = core.with_widget_read(
            node_id,
            WidgetOperation::layout("layout refresh"),
            |widget, _core| widget.layout(),
        )?;
        layout.validate()?;
        if let Some(node) = core.nodes.get_mut(node_id) {
            node.layout = layout;
            node.layout_dirty = false;
        }
    }
    Ok(())
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
    pub(super) fn new(core: &'a mut Core) -> Self {
        Self {
            core,
            measure_cache: HashMap::new(),
        }
    }

    /// Lay out a node subtree and return its outer size.
    fn layout_node(
        &mut self,
        node_id: NodeId,
        available_outer: Size,
        position: Point,
        parent_overflow: Overflow,
    ) -> Result<Size<u32>> {
        let (layout, hidden) = self.node_layout_snapshot(node_id)?;
        if hidden || layout.display == Display::None {
            self.clear_layout(node_id, position)?;
            return Ok(Size::ZERO);
        }

        let mut effective_layout = layout;
        effective_layout.inherit_overflow(parent_overflow.x, parent_overflow.y);

        let outer =
            self.resolve_outer_size_with_layout(node_id, effective_layout, available_outer)?;
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let content_size = Size::new(outer.w.saturating_sub(pad_x), outer.h.saturating_sub(pad_y));

        {
            let node = self
                .core
                .nodes
                .get_mut(node_id)
                .ok_or(Error::NodeNotFound(node_id))?;
            node.rect = Rect::new(position.x, position.y, outer.w, outer.h);
            node.content_size = content_size;
        }

        self.layout_children(node_id, effective_layout, content_size)?;

        let canvas = self.compute_canvas(node_id, content_size)?;
        self.update_canvas(node_id, content_size, canvas);

        Ok(outer)
    }

    /// Update view rectangles for a subtree based on parent view data.
    fn update_views(&mut self, node_id: NodeId, parent_view: View) -> Result<()> {
        let (layout, hidden, rect, content_size, canvas, scroll, children) = {
            let node = self
                .core
                .nodes
                .get(node_id)
                .ok_or(Error::NodeNotFound(node_id))?;
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
            return Ok(());
        }

        let outer_x = i64::from(parent_view.content.tl.x) + i64::from(rect.tl.x)
            - i64::from(parent_view.tl.x);
        let outer_y = i64::from(parent_view.content.tl.y) + i64::from(rect.tl.y)
            - i64::from(parent_view.tl.y);

        let outer = RectI32::new(
            clamp_i64_to_i32(outer_x),
            clamp_i64_to_i32(outer_y),
            rect.w,
            rect.h,
        );

        let content_x = i64::from(outer.tl.x) + i64::from(layout.padding.left);
        let content_y = i64::from(outer.tl.y) + i64::from(layout.padding.top);
        let content = RectI32::new(
            clamp_i64_to_i32(content_x),
            clamp_i64_to_i32(content_y),
            content_size.w,
            content_size.h,
        );

        let view = View::new(outer, content, scroll, canvas);
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            node.view = view;
        } else {
            return Err(Error::NodeNotFound(node_id));
        }

        for child in children {
            self.update_views(child, view)?;
        }

        Ok(())
    }

    /// Resolve a node's outer size using an explicit layout snapshot.
    fn resolve_outer_size_with_layout(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        available_outer: Size,
    ) -> Result<Size<u32>> {
        let available: Size<u32> = available_outer;
        let pad_x = layout.padding.horizontal();
        let pad_y = layout.padding.vertical();
        let available_content_w = available.w.saturating_sub(pad_x);
        let available_content_h = available.h.saturating_sub(pad_y);

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
            let m0 = self.measure_cached(node_id, c0)?;
            let raw0 = match m0 {
                Measurement::Fixed(content) => content,
                Measurement::Wrap => self.measure_wrap_content(node_id, layout, c0)?,
            };
            measured_content = c0.clamp_size(raw0);
        }

        let outer_w0 = match layout.width {
            Sizing::Flex(_) => available.w,
            Sizing::Measure => measured_content.w.saturating_add(pad_x),
        };
        let outer_h0 = match layout.height {
            Sizing::Flex(_) => available.h,
            Sizing::Measure => measured_content.h.saturating_add(pad_y),
        };

        let mut outer = Size::new(outer_w0, outer_h0);
        outer = clamp_outer(outer, layout);

        let mut content = Size::new(outer.w.saturating_sub(pad_x), outer.h.saturating_sub(pad_y));

        if did_measure {
            let width_seen = match c0.width {
                Constraint::Exact(n) => n,
                Constraint::AtMost(_) | Constraint::Unbounded => measured_content.w,
            };

            if content.w != width_seen {
                let c1 = MeasureConstraints {
                    width: Constraint::Exact(content.w),
                    height: c0.height,
                };
                let m1 = self.measure_cached(node_id, c1)?;
                let raw1 = match m1 {
                    Measurement::Fixed(content) => content,
                    Measurement::Wrap => self.measure_wrap_content(node_id, layout, c1)?,
                };
                let content1 = c1.clamp_size(raw1);

                if matches!(layout.height, Sizing::Measure) {
                    let outer_h1 = content1.h.saturating_add(pad_y);
                    outer.h = outer_h1;
                    outer = clamp_outer(outer, layout);
                    content =
                        Size::new(outer.w.saturating_sub(pad_x), outer.h.saturating_sub(pad_y));
                }
            }

            let c_final = MeasureConstraints {
                width: Constraint::Exact(content.w),
                height: Constraint::Exact(content.h),
            };
            self.measure_cached(node_id, c_final)?;
        }

        Ok(outer)
    }

    /// Measure content size by wrapping children when requested.
    fn measure_wrap_content(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        constraints: MeasureConstraints,
    ) -> Result<Size<u32>> {
        let children = self.visible_children(node_id)?;
        if children.is_empty() {
            return Ok(Size::ZERO);
        }

        // For Stack direction, content size is the max of all children
        if layout.direction == LayoutDirection::Stack {
            return self.measure_wrap_content_stack(layout, constraints, &children);
        }

        let main_fixed = constraints.main_is_exact(layout.direction);
        let cross_fixed = constraints.cross_is_exact(layout.direction);
        let avail_main = constraints.main(layout.direction).max_bound();
        let avail_cross = constraints.cross(layout.direction).max_bound();
        let avail = layout
            .direction
            .size_from_main_cross(avail_main, avail_cross);

        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut child_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child)?.0;
            let mut effective = child_layout;

            let child_main = main_sizing(child_layout, layout.direction);
            if !main_fixed && matches!(child_main, Sizing::Flex(_)) {
                set_main_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            let child_cross = cross_sizing(child_layout, layout.direction);
            if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
            }

            effective.inherit_overflow(layout.overflow_x, layout.overflow_y);

            let eff_main = main_sizing(effective, layout.direction);
            if let Sizing::Flex(w) = eff_main {
                flex_children.push((i, w));
                continue;
            }

            let size = self.resolve_outer_size_with_layout(*child, effective, avail)?;
            child_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(layout.direction.main_size(size));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = avail_main.saturating_sub(fixed_main_total.saturating_add(gap_total));

        if main_fixed && !flex_children.is_empty() {
            let weights: Vec<u32> = flex_children.iter().map(|(_, w)| *w).collect();
            let shares = allocate_flex_shares(remaining, &weights);
            for (idx, (child_index, _)) in flex_children.iter().enumerate() {
                let child_layout = self.node_layout_snapshot(children[*child_index])?.0;
                let mut effective = child_layout;
                let child_cross = cross_sizing(child_layout, layout.direction);
                if !cross_fixed && matches!(child_cross, Sizing::Flex(_)) {
                    set_cross_sizing(&mut effective, layout.direction, Sizing::Measure);
                }
                effective.inherit_overflow(layout.overflow_x, layout.overflow_y);
                let child_available = layout
                    .direction
                    .size_from_main_cross(shares[idx], avail_cross);
                let size = self.resolve_outer_size_with_layout(
                    children[*child_index],
                    effective,
                    child_available,
                )?;
                child_sizes[*child_index] = size;
            }
        }

        let mut main_total = 0u32;
        let mut cross_max = 0u32;
        for size in &child_sizes {
            main_total = main_total.saturating_add(layout.direction.main_size(*size));
            cross_max = cross_max.max(layout.direction.cross_size(*size));
        }
        main_total = main_total.saturating_add(gap_total);

        let content = layout.direction.size_from_main_cross(main_total, cross_max);
        Ok(constraints.clamp_size(content))
    }

    /// Measure content size for Stack direction - max of all children sizes.
    fn measure_wrap_content_stack(
        &mut self,
        layout: Layout,
        constraints: MeasureConstraints,
        children: &[NodeId],
    ) -> Result<Size<u32>> {
        let avail_w = constraints.width.max_bound();
        let avail_h = constraints.height.max_bound();
        let avail = Size::new(avail_w, avail_h);

        let mut max_w = 0u32;
        let mut max_h = 0u32;

        for child in children {
            let child_layout = self.node_layout_snapshot(*child)?.0;
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

            effective.inherit_overflow(layout.overflow_x, layout.overflow_y);

            let size = self.resolve_outer_size_with_layout(*child, effective, avail)?;
            max_w = max_w.max(size.w);
            max_h = max_h.max(size.h);
        }

        let content = Size::new(max_w, max_h);
        Ok(constraints.clamp_size(content))
    }

    /// Lay out visible children inside the provided content box.
    fn layout_children(
        &mut self,
        node_id: NodeId,
        layout: Layout,
        content: Size<u32>,
    ) -> Result<()> {
        let children = self.visible_children(node_id)?;
        if children.is_empty() {
            return Ok(());
        }

        let parent_overflow = Overflow::from_layout(layout);
        match layout.direction {
            LayoutDirection::Stack => {
                // Stack: all children get full content area, positioned according to alignment
                for child in &children {
                    // First, layout the child to determine its size
                    self.layout_node(*child, content, Point::zero(), parent_overflow)?;

                    // Then apply alignment to position the child within content area
                    let child_size = self.node_size(*child)?;
                    let offset_x = align_offset(child_size.w, content.w, layout.align_horizontal);
                    let offset_y = align_offset(child_size.h, content.h, layout.align_vertical);
                    self.set_node_position(
                        *child,
                        Point {
                            x: offset_x,
                            y: offset_y,
                        },
                    )?;
                }
            }
            LayoutDirection::Row | LayoutDirection::Column => {
                self.layout_children_sequential(layout, content, &children, parent_overflow)?;
            }
        }
        Ok(())
    }

    /// Layout children sequentially (Row or Column direction).
    fn layout_children_sequential(
        &mut self,
        layout: Layout,
        content: Size<u32>,
        children: &[NodeId],
        parent_overflow: Overflow,
    ) -> Result<()> {
        let mut fixed_main_total = 0u32;
        let mut flex_children: Vec<(usize, u32)> = Vec::new();
        let mut pre_sizes = vec![Size::ZERO; children.len()];

        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child)?.0;
            let main = main_sizing(child_layout, layout.direction);
            if let Sizing::Flex(w) = main {
                flex_children.push((i, w));
                continue;
            }

            let mut effective = child_layout;
            effective.inherit_overflow(parent_overflow.x, parent_overflow.y);

            let child_available = content;
            let size = self.resolve_outer_size_with_layout(*child, effective, child_available)?;
            pre_sizes[i] = size;
            fixed_main_total = fixed_main_total.saturating_add(layout.direction.main_size(size));
        }

        let gap_total = layout
            .gap
            .saturating_mul(children.len().saturating_sub(1) as u32);
        let remaining = layout
            .direction
            .main_size(content)
            .saturating_sub(fixed_main_total.saturating_add(gap_total));

        let weights: Vec<u32> = flex_children.iter().map(|(_, w)| *w).collect();
        let shares = allocate_flex_shares(remaining, &weights);

        let mut flex_idx = 0usize;
        let mut actual_sizes = Vec::with_capacity(children.len());
        for (i, child) in children.iter().enumerate() {
            let child_layout = self.node_layout_snapshot(*child)?.0;
            let mut effective = child_layout;
            effective.inherit_overflow(parent_overflow.x, parent_overflow.y);

            let main = match main_sizing(effective, layout.direction) {
                Sizing::Flex(_) => {
                    let share = shares[flex_idx];
                    flex_idx += 1;
                    share
                }
                Sizing::Measure => layout.direction.main_size(pre_sizes[i]),
            };

            let child_available = layout
                .direction
                .size_from_main_cross(main, layout.direction.cross_size(content));
            let actual =
                self.layout_node(*child, child_available, Point::zero(), parent_overflow)?;
            actual_sizes.push(actual);
        }

        let children_main = actual_sizes.iter().fold(0u32, |total, size| {
            total.saturating_add(layout.direction.main_size(*size))
        });
        let group_main = children_main.saturating_add(gap_total);
        let available_main = layout.direction.main_size(content);
        let available_cross = layout.direction.cross_size(content);
        let mut pos_main = align_offset(group_main, available_main, main_alignment(layout));

        for (child, actual) in children.iter().zip(actual_sizes) {
            let cross = align_offset(
                layout.direction.cross_size(actual),
                available_cross,
                cross_alignment(layout),
            );
            let position = match layout.direction {
                LayoutDirection::Row => Point {
                    x: pos_main,
                    y: cross,
                },
                LayoutDirection::Column => Point {
                    x: cross,
                    y: pos_main,
                },
                LayoutDirection::Stack => unreachable!(),
            };
            self.set_node_position(*child, position)?;
            pos_main = pos_main
                .saturating_add(layout.direction.main_size(actual))
                .saturating_add(layout.gap);
        }
        Ok(())
    }

    /// Get a node's outer size.
    fn node_size(&self, node_id: NodeId) -> Result<Size<u32>> {
        self.core
            .nodes
            .get(node_id)
            .map(|n| Size::new(n.rect.w, n.rect.h))
            .ok_or(Error::NodeNotFound(node_id))
    }

    /// Set a node's position within its parent's content area.
    fn set_node_position(&mut self, node_id: NodeId, position: Point) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        node.rect.tl = position;
        Ok(())
    }

    /// Compute the scrollable canvas size for a node.
    pub(super) fn compute_canvas(
        &self,
        node_id: NodeId,
        view_size: Size<u32>,
    ) -> Result<Size<u32>> {
        let children = self.visible_children(node_id).map_err(|error| {
            self.core
                .widget_operation_error(WidgetOperation::layout("canvas"), node_id, error)
        })?;
        let mut canvas_children = Vec::with_capacity(children.len());
        for child in children {
            let node = self
                .core
                .nodes
                .get(child)
                .ok_or(Error::NodeNotFound(child))
                .map_err(|error| {
                    self.core.widget_operation_error(
                        WidgetOperation::layout("canvas"),
                        node_id,
                        error,
                    )
                })?;
            let child_canvas: Size<u32> = node.canvas;
            canvas_children.push(CanvasChild::new(node.rect, child_canvas));
        }
        let ctx = CanvasContext::new(&canvas_children);
        let canvas = self.core.with_widget_read(
            node_id,
            WidgetOperation::layout("canvas"),
            |widget, _core| widget.canvas(view_size, &ctx),
        )?;
        Ok(Size::new(
            canvas.w.max(view_size.w),
            canvas.h.max(view_size.h),
        ))
    }

    /// Store the canvas size compute_canvas returned and clamp the scroll offset.
    fn update_canvas(&mut self, node_id: NodeId, view_size: Size<u32>, canvas: Size<u32>) {
        if let Some(node) = self.core.nodes.get_mut(node_id) {
            let mut scroll = node.scroll;
            clamp_scroll(&mut scroll, view_size, canvas);
            node.scroll = scroll;
            node.canvas = canvas;
        }
    }

    /// Snapshot a node's layout and hidden state.
    fn node_layout_snapshot(&self, node_id: NodeId) -> Result<(Layout, bool)> {
        self.core
            .nodes
            .get(node_id)
            .map(|node| (node.layout, node.hidden))
            .ok_or(Error::NodeNotFound(node_id))
    }

    /// Collect visible child nodes in tree order.
    fn visible_children(&self, node_id: NodeId) -> Result<Vec<NodeId>> {
        let node = self
            .core
            .nodes
            .get(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        let mut visible = Vec::new();
        for child in &node.children {
            let child_node = self
                .core
                .nodes
                .get(*child)
                .ok_or(Error::NodeNotFound(*child))?;
            if !child_node.hidden && child_node.layout.display == Display::Block {
                visible.push(*child);
            }
        }
        Ok(visible)
    }

    /// Get a cached measurement or compute and store it for this pass.
    pub(super) fn measure_cached(
        &mut self,
        node_id: NodeId,
        constraints: MeasureConstraints,
    ) -> Result<Measurement> {
        let key = MeasureKey {
            node: node_id,
            constraints,
        };
        if let Some(m) = self.measure_cache.get(&key) {
            return Ok(*m);
        }
        let measured = self.core.with_widget_read(
            node_id,
            WidgetOperation::layout("measure"),
            |widget, _core| widget.measure(constraints),
        )?;
        self.measure_cache.insert(key, measured);
        Ok(measured)
    }

    /// Reset layout data for a hidden subtree.
    fn clear_layout(&mut self, node_id: NodeId, position: Point) -> Result<()> {
        let node = self
            .core
            .nodes
            .get_mut(node_id)
            .ok_or(Error::NodeNotFound(node_id))?;
        node.rect = Rect::new(position.x, position.y, 0, 0);
        node.content_size = Size::default();
        node.canvas = Size::default();
        node.scroll = Point::zero();
        node.view = View::default();
        let children = self
            .core
            .nodes
            .get(node_id)
            .map(|node| node.children.clone())
            .ok_or(Error::NodeNotFound(node_id))?;
        for child in children {
            self.clear_layout(child, Point::zero())?;
        }
        Ok(())
    }
}

/// Clamp an outer size against min/max constraints.
fn clamp_outer(size: Size<u32>, layout: Layout) -> Size<u32> {
    Size::new(
        clamp_axis(size.w, layout.min_width, layout.max_width),
        clamp_axis(size.h, layout.min_height, layout.max_height),
    )
}

/// Clamp a single axis against optional min/max bounds.
fn clamp_axis(value: u32, min: Option<u32>, max: Option<u32>) -> u32 {
    debug_assert!(
        !matches!((min, max), (Some(min), Some(max)) if min > max),
        "Layout::validate rejects min above max before layout runs"
    );
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
pub fn clamp_scroll(scroll: &mut Point, view: Size<u32>, canvas: Size<u32>) {
    let max_x = if view.w == 0 {
        0
    } else {
        canvas.w.saturating_sub(view.w)
    };
    let max_y = if view.h == 0 {
        0
    } else {
        canvas.h.saturating_sub(view.h)
    };
    scroll.x = scroll.x.min(max_x);
    scroll.y = scroll.y.min(max_y);
}

/// Allocate remaining space proportionally across flex weights.
fn allocate_flex_shares(remaining: u32, weights: &[u32]) -> Vec<u32> {
    if remaining == 0 || weights.is_empty() {
        return vec![0; weights.len()];
    }
    let total: u64 = weights.iter().map(|w| u64::from(*w)).sum();
    if total == 0 {
        return vec![0; weights.len()];
    }

    let mut base = Vec::with_capacity(weights.len());
    let mut rem = Vec::with_capacity(weights.len());
    for w in weights {
        let weight = u64::from(*w);
        let prod = u64::from(remaining) * weight;
        base.push(u32::try_from(prod / total).unwrap_or(u32::MAX));
        rem.push(u32::try_from(prod % total).unwrap_or(u32::MAX));
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

/// Return the alignment controlling a sequential layout's child group.
fn main_alignment(layout: Layout) -> Align {
    match layout.direction {
        LayoutDirection::Row => layout.align_horizontal,
        LayoutDirection::Column => layout.align_vertical,
        LayoutDirection::Stack => unreachable!(),
    }
}

/// Return the alignment controlling each sequential child's cross axis.
fn cross_alignment(layout: Layout) -> Align {
    match layout.direction {
        LayoutDirection::Row => layout.align_vertical,
        LayoutDirection::Column => layout.align_horizontal,
        LayoutDirection::Stack => unreachable!(),
    }
}

/// Clamp a widened coordinate to the signed view coordinate domain.
fn clamp_i64_to_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    })
}

/// Depth-first search for a node at a screen-space point.
fn locate_recursive(
    core: &Core,
    node_id: NodeId,
    point: Point,
    parent_clip: Rect,
) -> Result<Option<NodeId>> {
    let node = core
        .nodes
        .get(node_id)
        .ok_or_else(|| Error::Internal("missing node".into()))?;

    if node.hidden || node.layout.display == Display::None {
        return Ok(None);
    }

    let Some(outer_clip) = node.view.outer.intersect_rect(parent_clip) else {
        return Ok(None);
    };
    if !outer_clip.contains_point(point) {
        return Ok(None);
    }

    let Some(child_clip) = node.view.content.intersect_rect(parent_clip) else {
        return Ok(Some(node_id));
    };
    let children = node.children.clone();
    for child in children.into_iter().rev() {
        if let Some(hit) = locate_recursive(core, child, point, child_clip)? {
            return Ok(Some(hit));
        }
    }

    Ok(Some(node_id))
}

/// Tests for the layout driver.
#[cfg(test)]
mod tests;
