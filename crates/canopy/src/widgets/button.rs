//! Button widget.

use crate::{
    Context, ViewContext, command,
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
            selected: false,
        }
    }

    /// Build a button with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a button that dispatches a command when clicked.
    pub fn with_command(mut self, command: CommandCall) -> Self {
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

    /// Handle a mouse click event.
    fn handle_click(&mut self, ctx: &mut dyn Context, event: mouse::MouseEvent) -> Result<bool> {
        if event.button == mouse::Button::Left && event.action == mouse::Action::Down {
            self.press(ctx)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Sync the label text widget to the current label.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let label = self.label.clone();
        let _ = ctx.try_with_unique_descendant::<Text, _>(|text, _ctx| {
            text.set_raw(label);
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
        let box_id = ctx.add_child(Box::new().with_glyphs(self.glyphs).with_fill())?;
        let center_id = ctx.add_child_to(box_id, Center::new())?;
        ctx.add_child_to_keyed(center_id, "label", Text::new(self.label.clone()))?;
        Ok(())
    }

    fn render(&mut self, rndr: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("button");
        if self.selected {
            rndr.push_layer("selected");
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> EventOutcome {
        if let Event::Mouse(mouse_event) = event
            && matches!(self.handle_click(ctx, *mouse_event), Ok(true))
        {
            return EventOutcome::Handle;
        }
        EventOutcome::Ignore
    }

    fn name(&self) -> NodeName {
        NodeName::convert("button")
    }
}
