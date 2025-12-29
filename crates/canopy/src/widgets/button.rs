//! Button widget.

use crate::{
    Context, NodeId, ViewContext, command,
    commands::{CommandCall, CommandInvocation},
    derive_commands,
    error::Result,
    event::{Event, mouse},
    layout::Layout,
    render::Render,
    state::NodeName,
    widget::{EventOutcome, Widget},
    widgets::{
        Box, Center, Text,
        boxed::{BoxGlyphs, SINGLE},
        list::Selectable,
    },
};

/// Button widget that triggers a command when clicked.
pub struct Button {
    /// Button label.
    label: String,
    /// Command invocation to dispatch on click.
    command: Option<CommandInvocation>,
    /// Glyph set for the button border.
    glyphs: BoxGlyphs,
    /// Mounted box node ID.
    box_id: Option<NodeId>,
    /// Mounted label node ID.
    text_id: Option<NodeId>,
    /// Selection state for use in lists.
    selected: bool,
}

impl Selectable for Button {
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

#[derive_commands]
impl Button {
    /// Construct a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: None,
            glyphs: SINGLE,
            box_id: None,
            text_id: None,
            selected: false,
        }
    }

    /// Build a button with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a button that dispatches a command when clicked.
    pub fn with_command<T>(mut self, command: CommandCall<T>) -> Self {
        self.command = Some(command.invocation());
        self
    }

    /// Return the button label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Replace the button label.
    pub fn set_label(&mut self, ctx: &mut dyn Context, label: impl Into<String>) -> Result<()> {
        self.label = label.into();
        self.sync_label(ctx)
    }

    /// Trigger the button action.
    #[command]
    pub fn press(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(command) = self.command.as_ref() {
            ctx.dispatch_command(command)?;
        }
        Ok(())
    }

    /// Ensure the child widget tree is mounted.
    fn ensure_tree(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if self.box_id.is_some() && self.text_id.is_some() {
            return Ok(());
        }

        let box_id = ctx.add_orphan(Box::new().with_glyphs(self.glyphs).with_fill());
        let center_id = ctx.add_orphan(Center::new());
        let text_id = ctx.add_orphan(Text::new(self.label.clone()));

        ctx.mount_child_to(center_id, text_id)?;
        ctx.mount_child_to(box_id, center_id)?;
        ctx.mount_child_to(ctx.node_id(), box_id)?;

        self.box_id = Some(box_id);
        self.text_id = Some(text_id);
        Ok(())
    }

    /// Sync the label text widget to the current label.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let Some(text_id) = self.text_id else {
            return Ok(());
        };
        let label = self.label.clone();
        ctx.with_widget(text_id, |text: &mut Text, _ctx| {
            text.set_raw(label.clone());
            Ok(())
        })?;
        Ok(())
    }
}

impl Default for Button {
    fn default() -> Self {
        Self::new("")
    }
}

impl Widget for Button {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.ensure_tree(ctx)
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("button");
        if self.selected {
            rndr.push_layer("selected");
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        match event {
            Event::Mouse(m)
                if m.button == mouse::Button::Left && m.action == mouse::Action::Down =>
            {
                if self.press(ctx).is_ok() {
                    return EventOutcome::Handle;
                }
            }
            _ => {}
        }
        EventOutcome::Ignore
    }

    fn name(&self) -> NodeName {
        NodeName::convert("button")
    }
}
