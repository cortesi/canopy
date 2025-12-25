use std::{any::Any, time::Duration};

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
    error::Result,
    event::{Event, key, mouse},
    geom::{Expanse, Rect},
    layout::{AvailableSpace, Dimension, Display, FlexDirection, Size, Style},
    render::Render,
    widget::{EventOutcome, Widget},
    widgets::Root,
};

/// A focusable block that can split into children.
pub struct Block {
    /// Child blocks.
    children: Vec<NodeId>,
    /// True for horizontal layout.
    horizontal: bool,
    /// Flex grow factor for this block as a child.
    flex_grow: f32,
    /// Flex shrink factor for this block as a child.
    flex_shrink: f32,
}

#[derive_commands]
impl Block {
    /// Construct a block with the requested orientation.
    fn new(orientation: bool) -> Self {
        Self {
            children: vec![],
            horizontal: orientation,
            flex_grow: 1.0,
            flex_shrink: 1.0,
        }
    }

    /// Return true when the available area is too small to split.
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    /// Synchronize child layout styles and ordering.
    fn sync_layout(&self, c: &mut dyn Context) -> Result<()> {
        let node_id = c.node_id();
        c.set_children(node_id, self.children.clone())?;

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = if self.horizontal {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            };
        };
        c.with_style(node_id, &mut update_root)?;

        for child in &self.children {
            let mut flex = (1.0, 1.0);
            c.with_widget_mut(*child, &mut |widget, _ctx| {
                let any = widget as &mut dyn Any;
                if let Some(block) = any.downcast_mut::<Self>() {
                    flex = (block.flex_grow, block.flex_shrink);
                }
                Ok(())
            })?;
            c.with_style(*child, &mut |style| {
                style.flex_grow = flex.0;
                style.flex_shrink = flex.1;
                style.flex_basis = Dimension::Auto;
            })?;
        }

        Ok(())
    }

    /// Apply the current flex factors to this node's layout style.
    fn apply_flex(&self, c: &mut dyn Context) -> Result<()> {
        let grow = self.flex_grow;
        let shrink = self.flex_shrink;
        c.with_style(c.node_id(), &mut |style| {
            style.flex_grow = grow;
            style.flex_shrink = shrink;
            style.flex_basis = Dimension::Auto;
        })
    }

    /// Adjust flex factors by the requested deltas and apply the updated style.
    fn adjust_flex(
        &mut self,
        c: &mut dyn Context,
        grow_delta: f32,
        shrink_delta: f32,
    ) -> Result<()> {
        if let Some(view) = c.node_view(c.node_id())
            && (view.w <= 1 || view.h <= 1)
            && (grow_delta < 0.0 || shrink_delta > 0.0)
        {
            return Ok(());
        }

        let min = 0.0;
        self.flex_grow = (self.flex_grow + grow_delta).max(min);
        self.flex_shrink = (self.flex_shrink + shrink_delta).max(min);
        self.apply_flex(c)
    }

    #[command]
    /// Add a nested block if space permits.
    fn add(&mut self, c: &mut dyn Context) -> Result<()> {
        let first_child = self.children.first().copied();
        if let Some(child_id) = first_child
            && let Some(view) = c.node_view(child_id)
        {
            let size = Expanse::new(view.w, view.h);
            if self.size_limited(size) {
                return Ok(());
            }
        }

        if !self.children.is_empty() {
            let child = c.add(Box::new(Self::new(!self.horizontal)));
            self.children.push(child);
            self.sync_layout(c)?;
        }

        Ok(())
    }

    #[command]
    /// Split into two child blocks.
    fn split(&mut self, c: &mut dyn Context) -> Result<()> {
        let view = c.view();
        let size = Expanse::new(view.w, view.h);
        if !self.size_limited(size) {
            let left = c.add(Box::new(Self::new(!self.horizontal)));
            let right = c.add(Box::new(Self::new(!self.horizontal)));
            self.children = vec![left, right];
            self.sync_layout(c)?;
            c.focus_next(c.node_id());
        }
        Ok(())
    }

    #[command]
    /// Increase this block's flex grow coefficient.
    fn flex_grow_inc(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 0.5, 0.0)
    }

    #[command]
    /// Decrease this block's flex grow coefficient.
    fn flex_grow_dec(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, -0.5, 0.0)
    }

    #[command]
    /// Increase this block's flex shrink coefficient.
    fn flex_shrink_inc(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 0.0, 0.5)
    }

    #[command]
    /// Decrease this block's flex shrink coefficient.
    fn flex_shrink_dec(&mut self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 0.0, -0.5)
    }

    #[command]
    /// Focus this block.
    fn focus(&self, c: &mut dyn Context) -> Result<()> {
        c.set_focus(c.node_id());
        Ok(())
    }
}

impl Widget for Block {
    fn accept_focus(&mut self) -> bool {
        self.children.is_empty()
    }

    fn measure(
        &self,
        _known_dimensions: Size<Option<f32>>,
        _available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        Size {
            width: 0.0,
            height: 0.0,
        }
    }

    fn canvas_size(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        let width = known_dimensions
            .width
            .or_else(|| available_space.width.into_option())
            .unwrap_or(0.0);
        let height = known_dimensions
            .height
            .or_else(|| available_space.height.into_option())
            .unwrap_or(0.0);
        Size { width, height }
    }

    fn render(&mut self, r: &mut Render, _area: Rect, ctx: &dyn ViewContext) -> Result<()> {
        if self.children.is_empty() {
            let view = ctx.view();
            let bc = if ctx.is_focused() { "violet" } else { "blue" };
            if view.is_zero() {
                return Ok(());
            }
            r.fill(bc, view, '\u{2588}')?;

            let viewport = ctx.viewport();
            let screen = ctx.node_viewport(ctx.root_id()).unwrap_or(viewport);
            if viewport.tl.x > screen.tl.x {
                r.fill("black", Rect::new(view.tl.x, view.tl.y, 1, view.h), ' ')?;
            }
            if viewport.tl.y > screen.tl.y {
                r.fill("black", Rect::new(view.tl.x, view.tl.y, view.w, 1), ' ')?;
            }
        }
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn configure_style(&self, style: &mut Style) {
        style.min_size.width = Dimension::Points(1.0);
        style.min_size.height = Dimension::Points(1.0);
    }
}

/// Root node for the focus gym demo.
pub struct FocusGym {
    /// Root block id.
    root_block: Option<NodeId>,
}

impl Default for FocusGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl FocusGym {
    /// Construct a new focus gym.
    pub fn new() -> Self {
        Self { root_block: None }
    }

    /// Ensure the initial tree of blocks is built.
    fn ensure_tree(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.root_block.is_some() {
            return Ok(());
        }

        let root_block = c.add(Box::new(Block::new(true)));
        let left = c.add(Box::new(Block::new(false)));
        let right = c.add(Box::new(Block::new(false)));

        c.set_children(c.node_id(), vec![root_block])?;

        c.with_widget_mut(root_block, &mut |widget, ctx| {
            let any = widget as &mut dyn Any;
            let block = any.downcast_mut::<Block>().expect("block type mismatch");
            block.children = vec![left, right];
            block.sync_layout(ctx)
        })?;

        let mut update_root = |style: &mut Style| {
            style.display = Display::Flex;
            style.flex_direction = FlexDirection::Column;
        };
        c.with_style(c.node_id(), &mut update_root)?;

        let mut grow = |style: &mut Style| {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::Auto;
        };
        c.with_style(root_block, &mut grow)?;

        self.root_block = Some(root_block);
        Ok(())
    }

    /// Find the focused node in the subtree rooted at the given node.
    fn find_focused(
        &self,
        c: &dyn ViewContext,
        node: NodeId,
        parent: Option<NodeId>,
    ) -> Option<(Option<NodeId>, NodeId)> {
        if c.node_is_focused(node) {
            return Some((parent, node));
        }
        for child in c.children(node) {
            if let Some(found) = self.find_focused(c, child, Some(node)) {
                return Some(found);
            }
        }
        None
    }

    /// Collect focusable leaf nodes in preorder.
    fn collect_focusable(&self, c: &dyn ViewContext, node: NodeId, out: &mut Vec<NodeId>) {
        let children = c.children(node);
        if children.is_empty() {
            out.push(node);
            return;
        }
        for child in children {
            self.collect_focusable(c, child, out);
        }
    }

    #[command]
    /// Delete the currently focused block.
    fn delete_focused(&mut self, c: &mut dyn Context) -> Result<()> {
        self.ensure_tree(c)?;
        let Some(root_block) = self.root_block else {
            return Ok(());
        };
        let Some((parent, focused)) = self.find_focused(c, root_block, None) else {
            return Ok(());
        };
        let Some(parent_id) = parent else {
            return Ok(());
        };

        let mut before = Vec::new();
        self.collect_focusable(c, root_block, &mut before);
        let focus_index = before.iter().position(|id| *id == focused);

        c.with_widget_mut(parent_id, &mut |widget, ctx| {
            let any = widget as &mut dyn Any;
            if let Some(block) = any.downcast_mut::<Block>() {
                block.children.retain(|id| *id != focused);
                block.sync_layout(ctx)?;
            }
            Ok(())
        })?;

        let mut after = Vec::new();
        self.collect_focusable(c, root_block, &mut after);
        let target = focus_index
            .and_then(|idx| after.get(idx).copied())
            .or_else(|| after.last().copied());
        if let Some(target) = target {
            c.set_focus(target);
        } else {
            c.focus_first(c.root_id());
        }
        Ok(())
    }
}

impl Widget for FocusGym {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_event(&mut self, _event: &Event, _ctx: &mut dyn Context) -> EventOutcome {
        EventOutcome::Ignore
    }

    fn poll(&mut self, c: &mut dyn Context) -> Option<Duration> {
        self.ensure_tree(c).ok();
        None
    }
}

impl Loader for FocusGym {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<Block>();
    }
}

/// Install key bindings for the focus gym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    Binder::new(cnpy)
        .defaults::<Root>()
        .key('p', "print(\"xxxx\")")
        .with_path("focus_gym/")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .mouse(mouse::Action::ScrollDown, "root::focus_next()")
        .mouse(mouse::Action::ScrollUp, "root::focus_prev()")
        .key(key::KeyCode::Right, "root::focus_right()")
        .key('l', "root::focus_right()")
        .key(key::KeyCode::Left, "root::focus_left()")
        .key('h', "root::focus_left()")
        .key(key::KeyCode::Up, "root::focus_up()")
        .key('k', "root::focus_up()")
        .key(key::KeyCode::Down, "root::focus_down()")
        .key('j', "root::focus_down()")
        .key('x', "focus_gym::delete_focused()")
        .with_path("block")
        .key('s', "block::split()")
        .key('a', "block::add()")
        .key('[', "block::flex_grow_dec()")
        .key(']', "block::flex_grow_inc()")
        .key('{', "block::flex_shrink_dec()")
        .key('}', "block::flex_shrink_inc()")
        .mouse(mouse::Button::Left, "block::focus()")
        .mouse(mouse::Button::Middle, "block::split()")
        .mouse(mouse::Button::Right, "block::add()");
    Ok(())
}

#[cfg(test)]
mod tests {
    use canopy::{
        geom::{Expanse, Point},
        testing::harness::Harness,
    };

    use super::*;

    fn setup_harness(size: Expanse) -> Result<Harness> {
        let mut harness = Harness::builder(FocusGym::new())
            .size(size.w, size.h)
            .build()?;
        setup_bindings(&mut harness.canopy)?;
        harness.render()?;
        Ok(harness)
    }

    fn root_block_id(harness: &mut Harness) -> NodeId {
        harness.with_root_widget(|widget| {
            let any = widget as &mut dyn Any;
            let root = any.downcast_mut::<FocusGym>().expect("root type mismatch");
            root.root_block.expect("root block not initialized")
        })
    }

    macro_rules! find_separator_column {
        ($buf:expr, $left_view:expr, $right_view:expr) => {{
            let buf = $buf;
            let left_view = $left_view;
            let right_view = $right_view;
            let start_x = left_view.tl.x;
            let end_x = right_view.tl.x;
            let mut found = None;
            for x in start_x..=end_x {
                let mut all_space = true;
                let mut has_neighbors = false;
                for y in 0..buf.size().h {
                    let cell = buf.get(Point { x, y }).unwrap();
                    if cell.ch != ' ' {
                        all_space = false;
                        break;
                    }
                    let left_ok = x > 0
                        && buf
                            .get(Point { x: x - 1, y })
                            .is_some_and(|c| c.ch == '\u{2588}');
                    let right_ok = x + 1 < buf.size().w
                        && buf
                            .get(Point { x: x + 1, y })
                            .is_some_and(|c| c.ch == '\u{2588}');
                    if left_ok && right_ok {
                        has_neighbors = true;
                    }
                }
                if all_space && has_neighbors {
                    found = Some(x);
                    break;
                }
            }
            found
        }};
    }

    #[test]
    fn test_initial_render_draws_blocks() -> Result<()> {
        let harness = setup_harness(Expanse::new(40, 12))?;
        let buf = harness.buf();
        let size = buf.size();
        let mut found = false;
        for y in 0..size.h {
            for x in 0..size.w {
                let cell = buf.get(Point { x, y }).unwrap();
                if cell.ch == '\u{2588}' {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        assert!(found, "expected initial render to draw focus blocks");
        Ok(())
    }

    #[test]
    fn test_horizontal_children_fill_height() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let parent = core.nodes[root_block].viewport;
        let children = core.nodes[root_block].children.clone();
        assert_eq!(children.len(), 2);
        for child in children {
            let vp = core.nodes[child].viewport;
            assert_eq!(vp.h, parent.h);
            assert_eq!(vp.tl.y, parent.tl.y);
        }
        Ok(())
    }

    #[test]
    fn test_vertical_children_fill_width_and_height() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");

        harness.key('s')?;

        let core = &harness.canopy.core;
        let parent = core.nodes[left].viewport;
        let children = core.nodes[left].children.clone();
        assert_eq!(children.len(), 2);
        let mut max_bottom = parent.tl.y;
        for child in children {
            let vp = core.nodes[child].viewport;
            assert_eq!(vp.w, parent.w);
            max_bottom = max_bottom.max(vp.tl.y + vp.h);
        }
        assert_eq!(max_bottom, parent.tl.y + parent.h);
        Ok(())
    }

    #[test]
    fn test_flex_grow_and_shrink_commands_update_style() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");

        let grow_before = core.nodes[left].style.flex_grow;
        let shrink_before = core.nodes[left].style.flex_shrink;

        harness.key(']')?;
        harness.key('}')?;

        let core = &harness.canopy.core;
        let grow_after = core.nodes[left].style.flex_grow;
        let shrink_after = core.nodes[left].style.flex_shrink;

        assert!(grow_after > grow_before);
        assert!(shrink_after > shrink_before);

        Ok(())
    }

    #[test]
    fn test_flex_grow_affects_layout() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");
        let right = core.nodes[root_block]
            .children
            .get(1)
            .copied()
            .expect("missing right child");

        let left_before = core.nodes[left].viewport.w;
        let right_before = core.nodes[right].viewport.w;
        assert_eq!(left_before, right_before);

        harness.key(']')?;

        let core = &harness.canopy.core;
        let left_after = core.nodes[left].viewport.w;
        let right_after = core.nodes[right].viewport.w;
        assert!(left_after > right_after);
        Ok(())
    }

    #[test]
    fn test_flex_adjust_refuses_at_min_size() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(2, 2))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");

        let view = core.nodes[left].vp.view();
        assert!(view.w <= 1 || view.h <= 1);

        let grow_before = core.nodes[left].style.flex_grow;
        let shrink_before = core.nodes[left].style.flex_shrink;

        harness.key('[')?;
        harness.key('}')?;

        let core = &harness.canopy.core;
        assert_eq!(core.nodes[left].style.flex_grow, grow_before);
        assert_eq!(core.nodes[left].style.flex_shrink, shrink_before);

        Ok(())
    }

    #[test]
    fn test_screen_edge_is_flush() -> Result<()> {
        let harness = setup_harness(Expanse::new(40, 12))?;
        let cell = harness.buf().get(Point { x: 0, y: 0 }).unwrap();
        assert_eq!(cell.ch, '\u{2588}');
        Ok(())
    }

    #[test]
    fn test_single_separator_between_root_children() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(40, 12))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");
        let left_view = core.nodes[left].viewport;
        let right = core.nodes[root_block]
            .children
            .get(1)
            .copied()
            .expect("missing right child");
        let right_view = core.nodes[right].viewport;

        harness.render()?;
        let buf = harness.buf();
        let separator = find_separator_column!(&buf, left_view, right_view);
        assert!(
            separator.is_some(),
            "expected a single-column separator between root children"
        );

        Ok(())
    }

    #[test]
    fn test_delete_focused_block() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&mut harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");
        let right = core.nodes[root_block]
            .children
            .get(1)
            .copied()
            .expect("missing right child");
        assert_eq!(core.focus, Some(left));

        harness.key('x')?;

        let core = &harness.canopy.core;
        assert_eq!(core.nodes[root_block].children.len(), 1);
        assert_eq!(core.focus, Some(right));
        Ok(())
    }

    #[test]
    fn test_separators_remain_continuous_after_nested_splits() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(40, 12))?;
        harness.key('s')?;

        let root_block = root_block_id(&mut harness);
        let right = harness.canopy.core.nodes[root_block]
            .children
            .get(1)
            .copied()
            .expect("missing right child");
        harness.canopy.core.set_focus(right);
        harness.key('s')?;

        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");
        let left_view = core.nodes[left].viewport;
        let right_view = core.nodes[right].viewport;

        harness.render()?;
        let buf = harness.buf();
        let boundary_x = find_separator_column!(&buf, left_view, right_view)
            .expect("expected a separator column for nested splits");
        for y in 0..buf.size().h {
            let cell = buf.get(Point { x: boundary_x, y }).unwrap();
            assert_eq!(cell.ch, ' ');
        }

        Ok(())
    }
}
