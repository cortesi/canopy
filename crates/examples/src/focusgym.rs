use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::{Expanse, Rect},
    layout::{AvailableSpace, Dimension, Layout, Size},
    render::Render,
    widget::Widget,
    widgets::Root,
};

/// A focusable block that can split into children.
pub struct Block {
    /// True for horizontal layout.
    horizontal: bool,
}

#[derive_commands]
impl Block {
    /// Construct a block with the requested orientation.
    fn new(horizontal: bool) -> Self {
        Self { horizontal }
    }

    /// Return true when the available area is too small to split.
    fn size_limited(&self, a: Expanse) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    /// Adjust flex factors by the requested deltas and apply the updated layout.
    fn adjust_flex(&self, c: &mut dyn Context, grow_delta: f32, shrink_delta: f32) -> Result<()> {
        if let Some(view) = c.node_view(c.node_id())
            && (view.w <= 1 || view.h <= 1)
            && (grow_delta < 0.0 || shrink_delta > 0.0)
        {
            return Ok(());
        }

        let layout = c.layout();
        let min = 0.0;
        let grow = (layout.get_flex_grow() + grow_delta).max(min);
        let shrink = (layout.get_flex_shrink() + shrink_delta).max(min);
        c.with_layout(&mut |layout| {
            layout.flex_item(grow, shrink, Dimension::Auto);
        })
    }

    #[command]
    /// Add a nested block if space permits.
    fn add(&self, c: &mut dyn Context) -> Result<()> {
        if let Some(first_child) = c.children().first().copied()
            && let Some(view) = c.node_view(first_child)
        {
            let size = Expanse::new(view.w, view.h);
            if self.size_limited(size) {
                return Ok(());
            }
            c.add_child(Self::new(!self.horizontal))?;
        }
        Ok(())
    }

    #[command]
    /// Split into two child blocks.
    fn split(&self, c: &mut dyn Context) -> Result<()> {
        let view = c.view();
        let size = Expanse::new(view.w, view.h);
        if !self.size_limited(size) && c.children().is_empty() {
            c.add_child(Self::new(!self.horizontal))?;
            c.add_child(Self::new(!self.horizontal))?;
            c.focus_next();
        }
        Ok(())
    }

    #[command]
    /// Increase this block's flex grow coefficient.
    fn flex_grow_inc(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 0.5, 0.0)
    }

    #[command]
    /// Decrease this block's flex grow coefficient.
    fn flex_grow_dec(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, -0.5, 0.0)
    }

    #[command]
    /// Increase this block's flex shrink coefficient.
    fn flex_shrink_inc(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 0.0, 0.5)
    }

    #[command]
    /// Decrease this block's flex shrink coefficient.
    fn flex_shrink_dec(&self, c: &mut dyn Context) -> Result<()> {
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
    fn accept_focus(&self, ctx: &dyn ViewContext) -> bool {
        ctx.children().is_empty()
    }

    /// Block has no intrinsic size; it relies entirely on flex layout.
    fn view_size(
        &self,
        _known_dimensions: Size<Option<f32>>,
        _available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        Size {
            width: 0.0,
            height: 0.0,
        }
    }

    /// Block's canvas matches its layout allocation (no scrolling).
    ///
    /// Unlike `view_size` which returns (0, 0) for flex layout, `canvas_size` must return the
    /// actual available space so viewport calculations work correctly.
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
        // Only render leaf blocks (those without children)
        if ctx.children().is_empty() {
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

    fn layout(&self, layout: &mut Layout) {
        if self.horizontal {
            layout.flex_row();
        } else {
            layout.flex_col();
        }
        layout
            .flex_item(1.0, 1.0, Dimension::Auto)
            .min_size(Dimension::Points(1.0), Dimension::Points(1.0));
    }
}

/// Root node for the focus gym demo.
#[derive(Default)]
pub struct FocusGym;

#[derive_commands]
impl FocusGym {
    /// Construct a new focus gym.
    pub fn new() -> Self {
        Self
    }

    #[command]
    /// Delete the currently focused block.
    fn delete_focused(&self, c: &mut dyn Context) -> Result<()> {
        let Some(root_block) = c.only_child() else {
            return Ok(());
        };
        let Some(focused) = c.focused_leaf(root_block) else {
            return Ok(());
        };
        let Some(parent_id) = c.parent_of(focused) else {
            return Ok(());
        };
        let target = c.suggest_focus_after_remove(root_block, focused);

        let mut children = c.children_of(parent_id);
        children.retain(|id| *id != focused);
        c.set_children_of(parent_id, children)?;

        if let Some(target) = target {
            c.set_focus(target);
        } else {
            c.focus_first_global();
        }
        Ok(())
    }
}

impl Widget for FocusGym {
    fn render(&mut self, _r: &mut Render, _area: Rect, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        c.with_layout(&mut |layout| {
            layout.flex_col();
        })?;
        let root_block = c.add_child(Block::new(true))?;
        c.add_child_to(root_block, Block::new(false))?;
        c.add_child_to(root_block, Block::new(false))?;
        Ok(())
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
        .key('p', "print(\"focus gym\")")
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
        NodeId,
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

    fn root_block_id(harness: &Harness) -> NodeId {
        harness
            .canopy
            .core
            .nodes
            .get(harness.root)
            .and_then(|node| node.children.first().copied())
            .expect("root block not initialized")
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
        let harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&harness);
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
        let root_block = root_block_id(&harness);
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
        let root_block = root_block_id(&harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");

        let grow_before = core.nodes[left].layout.get_flex_grow();
        let shrink_before = core.nodes[left].layout.get_flex_shrink();

        harness.key(']')?;
        harness.key('}')?;

        let core = &harness.canopy.core;
        let grow_after = core.nodes[left].layout.get_flex_grow();
        let shrink_after = core.nodes[left].layout.get_flex_shrink();

        assert!(grow_after > grow_before);
        assert!(shrink_after > shrink_before);

        Ok(())
    }

    #[test]
    fn test_flex_grow_affects_layout() -> Result<()> {
        let mut harness = setup_harness(Expanse::new(60, 14))?;
        let root_block = root_block_id(&harness);
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
        let root_block = root_block_id(&harness);
        let core = &harness.canopy.core;
        let left = core.nodes[root_block]
            .children
            .first()
            .copied()
            .expect("missing left child");

        let view = core.nodes[left].vp.view();
        assert!(view.w <= 1 || view.h <= 1);

        let grow_before = core.nodes[left].layout.get_flex_grow();
        let shrink_before = core.nodes[left].layout.get_flex_shrink();

        harness.key('[')?;
        harness.key('}')?;

        let core = &harness.canopy.core;
        assert_eq!(core.nodes[left].layout.get_flex_grow(), grow_before);
        assert_eq!(core.nodes[left].layout.get_flex_shrink(), shrink_before);

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
        let root_block = root_block_id(&harness);
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
        let root_block = root_block_id(&harness);
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

        let root_block = root_block_id(&harness);
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
