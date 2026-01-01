use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, Widget, command, derive_commands,
    error::Result,
    event::{key, mouse},
    geom::Expanse,
    layout::{Direction, Layout, Sizing},
    render::Render,
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
    fn adjust_flex(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if let Some(view) = c.node_view(c.node_id())
            && (view.outer.w <= 1 || view.outer.h <= 1)
            && delta < 0
        {
            return Ok(());
        }

        let parent_dir = if let Some(parent) = c.parent_of(c.node_id()) {
            let mut dir = None;
            c.with_layout_of(parent, &mut |layout| {
                dir = Some(layout.direction);
            })?;
            dir
        } else {
            None
        };

        let adjust_horizontal = match parent_dir {
            Some(Direction::Row) => true,
            Some(Direction::Column) => false,
            Some(Direction::Stack) | None => self.horizontal,
        };

        let layout = c.layout();
        let weight = if adjust_horizontal {
            match layout.width {
                Sizing::Flex(w) => w,
                _ => 1,
            }
        } else {
            match layout.height {
                Sizing::Flex(w) => w,
                _ => 1,
            }
        };
        let next = weight.saturating_add_signed(delta).max(1);
        c.with_layout(&mut |layout| {
            if adjust_horizontal {
                layout.width = Sizing::Flex(next);
            } else {
                layout.height = Sizing::Flex(next);
            }
        })
    }

    #[command]
    /// Add a nested block if space permits.
    fn add(&self, c: &mut dyn Context) -> Result<()> {
        if let Some(first_child) = c.children().first().copied()
            && let Some(view) = c.node_view(first_child)
        {
            let size = Expanse::new(view.outer.w, view.outer.h);
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
        let size = Expanse::new(view.outer.w, view.outer.h);
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
        self.adjust_flex(c, 1)
    }

    #[command]
    /// Decrease this block's flex grow coefficient.
    fn flex_grow_dec(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, -1)
    }

    #[command]
    /// Increase this block's flex shrink coefficient.
    fn flex_shrink_inc(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, 1)
    }

    #[command]
    /// Decrease this block's flex shrink coefficient.
    fn flex_shrink_dec(&self, c: &mut dyn Context) -> Result<()> {
        self.adjust_flex(c, -1)
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

    fn render(&mut self, r: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        // Only render leaf blocks (those without children)
        if ctx.children().is_empty() {
            let bc = if ctx.is_focused() { "violet" } else { "blue" };
            let rect = ctx.view().outer_rect_local();
            if rect.is_zero() {
                return Ok(());
            }
            r.fill(bc, rect, '\u{2588}')?;
        }
        Ok(())
    }

    fn layout(&self) -> Layout {
        let base = if self.horizontal {
            Layout::row()
        } else {
            Layout::column()
        };
        base.flex_horizontal(1)
            .flex_vertical(1)
            .min_width(1)
            .min_height(1)
            .gap(1)
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
        let Some(root_block) = c.unique_child::<Block>()? else {
            return Ok(());
        };
        let Some(focused) = c.focused_leaf(root_block.into()) else {
            return Ok(());
        };
        c.remove_subtree(focused)?;
        c.focus_first_in(root_block.into());
        Ok(())
    }
}

impl Widget for FocusGym {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        c.with_layout(&mut |layout| {
            *layout = Layout::column().flex_horizontal(1).flex_vertical(1);
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
