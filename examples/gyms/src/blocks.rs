//! Markdown edited a block at a time, in place in the rendered view, with a
//! host that keeps its own copy of the text.

use std::time::Duration;

use canopy::{
    CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, Widget, derive_commands,
    error::Result, layout::Layout,
};
use canopy_markdown::{Markdown, MarkdownOptions, Mode};
use canopy_widgets::{
    Text,
    editor::{EditMode, EditorConfig},
};

use crate::{fixed_row, flex_row};

/// The document the gym edits.
pub const DOCUMENT: &str = include_str!("../fixtures/blocks.md");

/// The notice of the gym's lock on block edits.
const LOCKED: &str = "The gym locked the blocks: L unlocks them";

/// The gym's bindings, beside the reader's own.
const BINDINGS: &str = r#"
root.default_bindings()
markdown.default_bindings()
canopy.keymap({
    path = "blocks_gym",
    { key = "L", description = "Lock or unlock the blocks", action = command.blocks_gym.toggle_lock() },
})
"#;

canopy::slot!(ReaderSlot: Markdown);
canopy::slot!(StatusSlot: Text);

/// Access the mounted reader in its own context.
fn reader<R>(
    ctx: &mut dyn Context,
    f: impl FnOnce(&mut Markdown, &mut dyn Context) -> Result<R>,
) -> Result<R> {
    ctx.with_slot::<ReaderSlot, _>(ctx.node_id(), f)
}

/// A reader that rests in the block cursor and edits blocks in a vi editor,
/// beside a host copy of the text that takes every edit.
pub struct BlocksGym {
    /// The host's own copy of the text.
    text: String,
    /// The edits the host applied.
    applied: usize,
    /// Whether the host locks the blocks.
    locked: bool,
}

impl Default for BlocksGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl BlocksGym {
    /// Construct the gym over its document.
    pub fn new() -> Self {
        Self {
            text: DOCUMENT.to_owned(),
            applied: 0,
            locked: false,
        }
    }

    /// Apply the reader's edits to the host's copy, and show the count.
    fn take_edits(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let edits = reader(ctx, |md, _| Ok(md.take_edits()))?;
        if edits.is_empty() {
            return Ok(());
        }
        for edit in edits {
            self.text.replace_range(edit.range, &edit.text);
            self.applied += 1;
        }
        self.show_status(ctx)
    }

    /// Show what the host holds.
    fn show_status(&self, ctx: &mut dyn Context) -> Result<()> {
        let text = format!(
            "Blocks · e edits the marked block · Esc commits · ctrl-x discards · L locks · {} edits applied{}",
            self.applied,
            if self.locked { " · locked" } else { "" }
        );
        ctx.with_slot::<StatusSlot, _>(ctx.node_id(), |status, _| {
            status.set_text(text);
            Ok(())
        })
    }

    #[command]
    /// Whether the reader shows its current text.
    pub fn ready(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, _| Ok(md.ready()))
    }
    #[command]
    /// Focus the reader's view.
    pub fn focus_reader(&self, ctx: &mut dyn Context) -> Result<()> {
        reader(ctx, |md, ctx| {
            if let Some(node) = md.navigation_node() {
                ctx.set_focus(node)?;
            }
            Ok(())
        })
    }
    #[command]
    /// Whether the block editor is open.
    pub fn editing(&self, ctx: &mut dyn Context) -> Result<bool> {
        reader(ctx, |md, _| Ok(md.block_open()))
    }
    #[command]
    /// Whether the host's copy of the text equals the reader's.
    pub fn in_sync(&mut self, ctx: &mut dyn Context) -> Result<bool> {
        self.take_edits(ctx)?;
        let source = reader(ctx, |md, _| Ok(md.accepted_source()))?;
        Ok(*source == *self.text)
    }
    #[command]
    /// The number of edits the host applied.
    pub fn applied(&self) -> usize {
        self.applied
    }
    #[command]
    /// Lock the blocks, as a host does while it waits for a voice edit, or
    /// unlock them.
    pub fn toggle_lock(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.locked = !self.locked;
        let notice = self.locked.then(|| LOCKED.to_owned());
        reader(ctx, |md, _| {
            md.lock_blocks(notice);
            Ok(())
        })?;
        self.show_status(ctx)
    }
}

impl Widget for BlocksGym {
    fn name(&self) -> NodeName {
        NodeName::convert("blocks_gym")
    }
    fn layout(&self) -> Layout {
        Layout::fill()
    }
    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let status =
            ctx.add_slot::<StatusSlot>(ctx.node_id(), Text::new("").with_style("markdown/status"))?;
        ctx.set_layout_override(status.into(), fixed_row(1))?;
        let options = MarkdownOptions {
            resting_mode: Mode::Interact,
            block_editor: Some(EditorConfig::new().with_mode(EditMode::Vi)),
            coalesce: Duration::ZERO,
            ..MarkdownOptions::default()
        };
        let node =
            ctx.add_slot::<ReaderSlot>(ctx.node_id(), Markdown::new(self.text.clone(), options)?)?;
        ctx.set_layout_override(node.into(), flex_row(1))?;
        self.show_status(ctx)
    }
    fn poll(&mut self, ctx: &mut dyn Context) -> Result<Option<Duration>> {
        self.take_edits(ctx)?;
        Ok(Some(Duration::from_millis(16)))
    }
}

impl Register for BlocksGym {
    fn register(setup: &mut Setup) -> Result<()> {
        Markdown::register(setup)?;
        setup.add_commands::<Self>()
    }
}

/// Add the gym's default bindings.
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.script("blocks", BINDINGS)
}
