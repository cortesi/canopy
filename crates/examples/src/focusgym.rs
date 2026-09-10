use canopy::{CanopyBuilder, derive_commands, error::Result, prelude::*};
use canopy_widgets::Root;

/// Default bindings for the focus gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind("p", { phase = "after_widget", description = "Log demo message" }, function()
    canopy.log("focus gym")
end)
canopy.bind_command("Tab", { phase = "after_widget", path = "focus_gym", description = "Next focus" }, "root::focus", "Next")
canopy.bind_mouse("ScrollDown", { phase = "after_widget", path = "focus_gym", description = "Next focus" }, function()
    root.focus("Next")
end)
canopy.bind_mouse("ScrollUp", { phase = "after_widget", path = "focus_gym", description = "Previous focus" }, function()
    root.focus("Prev")
end)
canopy.bind_command("Right", { phase = "after_widget", path = "focus_gym", description = "Focus right" }, "root::focus", "Right")
canopy.bind_command("l", { phase = "after_widget", path = "focus_gym", description = "Focus right" }, "root::focus", "Right")
canopy.bind_command("Left", { phase = "after_widget", path = "focus_gym", description = "Focus left" }, "root::focus", "Left")
canopy.bind_command("h", { phase = "after_widget", path = "focus_gym", description = "Focus left" }, "root::focus", "Left")
canopy.bind_command("Up", { phase = "after_widget", path = "focus_gym", description = "Focus up" }, "root::focus", "Up")
canopy.bind_command("k", { phase = "after_widget", path = "focus_gym", description = "Focus up" }, "root::focus", "Up")
canopy.bind_command("Down", { phase = "after_widget", path = "focus_gym", description = "Focus down" }, "root::focus", "Down")
canopy.bind_command("j", { phase = "after_widget", path = "focus_gym", description = "Focus down" }, "root::focus", "Down")
canopy.bind_command("x", { phase = "after_widget", path = "focus_gym", description = "Delete focused block" }, "focus_gym::delete_focused")
canopy.bind_command("s", { phase = "before_widget", path = "block", description = "Split block" }, "block::split")
canopy.bind_command("a", { phase = "before_widget", path = "block", description = "Add child block" }, "block::add")
canopy.bind_command("[", { phase = "before_widget", path = "block", description = "Decrease grow" }, "block::flex_grow_dec")
canopy.bind_command("]", { phase = "before_widget", path = "block", description = "Increase grow" }, "block::flex_grow_inc")
canopy.bind_mouse("LeftDown", { phase = "after_widget", path = "block", description = "Focus block" }, function()
    block.focus()
end)
canopy.bind_mouse("MiddleDown", { phase = "after_widget", path = "block", description = "Split block" }, function()
    block.split()
end)
canopy.bind_mouse("RightDown", { phase = "after_widget", path = "block", description = "Add child block" }, function()
    block.add()
end)
"#;

/// A focusable block that can split into children.
pub(crate) struct Block {
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
    fn size_limited(&self, a: Size) -> bool {
        (self.horizontal && a.w <= 4) || (!self.horizontal && a.h <= 4)
    }

    /// Adjust flex factors by the requested deltas and apply the updated
    /// layout.
    fn adjust_flex(&self, c: &mut dyn Context, delta: i32) -> Result<()> {
        if let Some(view) = c.view_of(c.node_id())
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
            && let Some(view) = c.view_of(first_child)
        {
            let size = view.outer.size();
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
        let size = view.outer.size();
        if !self.size_limited(size) && c.children().is_empty() {
            c.add_child(Self::new(!self.horizontal))?;
            c.add_child(Self::new(!self.horizontal))?;
            c.focus_move(FocusScope::Current, FocusDirection::Next)?;
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
    /// Focus this block.
    fn focus(&self, c: &mut dyn Context) -> Result<()> {
        c.set_focus(c.node_id())?;
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
            if rect.is_empty() {
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
        let Some(root_block) = (c as &dyn ViewContext).unique_child::<Block>()? else {
            return Ok(());
        };
        let root_block = NodeId::from(root_block);
        let Some(focused) = c.focused_leaf(root_block) else {
            return Ok(());
        };
        if focused == root_block {
            return Ok(());
        }
        c.remove_subtree(focused)?;
        c.focus_first(FocusScope::Node(root_block))?;
        Ok(())
    }
}

impl Widget for FocusGym {
    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        c.set_layout(Layout::fill())?;
        let root_block = c.add_child(Block::new(true))?;
        c.add_child_to(root_block, Block::new(false))?;
        c.add_child_to(root_block, Block::new(false))?;
        Ok(())
    }
}

impl Loader for FocusGym {
    fn load(c: &mut Canopy) -> Result<()> {
        Root::load(c)?;
        c.add_commands::<Self>()?;
        c.add_commands::<Block>()?;
        Ok(())
    }
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("focusgym", DEFAULT_BINDINGS)
}
