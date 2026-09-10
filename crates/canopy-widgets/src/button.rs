//! Button widget.

use canopy::{
    Context, ContextExt, EventOutcome, ViewContext, Widget, WidgetSemantics,
    commands::{CommandAction, CommandCall, CommandStatus, CommandTarget},
    derive_commands,
    error::Result,
    event::{Event, mouse},
    layout::Layout,
    render::Render,
    state::NodeName,
    style::{WidgetState, roles},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    Border, Center, Text,
    boxed::{BoxGlyphs, SINGLE},
};

canopy::slot!(LabelSlot: Text);
canopy::slot!(BoxSlot: Border);
canopy::slot!(CenterSlot: Center);

/// Button widget that triggers a command when clicked.
pub struct Button {
    /// Button label.
    label: String,
    /// Command invocation to dispatch on click.
    command: Option<CommandAction>,
    /// Glyph set for the button border.
    glyphs: BoxGlyphs,
    /// Active state for the button.
    active: bool,
}

#[derive_commands]
impl Button {
    /// Construct a new button with a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: None,
            glyphs: SINGLE,
            active: false,
        }
    }

    /// Build a button with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a button that dispatches a command when clicked.
    pub fn with_command(mut self, command: CommandCall) -> Self {
        self.command = Some(command.action());
        self
    }

    /// Set whether the button is active.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Trigger the button action.
    #[command]
    pub fn press(&mut self, ctx: &mut dyn Context) -> Result<()> {
        if let Some(command) = self.command.as_ref() {
            ctx.dispatch(
                command.target.unwrap_or(CommandTarget::From(ctx.node_id())),
                &command.invocation,
            )?;
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

    /// Compute the label width in terminal cells.
    fn label_width(&self) -> u32 {
        self.label
            .lines()
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0)
            .max(1) as u32
    }

    /// Read command eligibility without conflating it with the active state.
    fn command_status(&self, ctx: &dyn ViewContext) -> Result<Option<CommandStatus>> {
        if self.command.is_some() && !ctx.is_attached_of(ctx.node_id()) {
            return Ok(Some(CommandStatus::Disabled("Button is detached".into())));
        }
        self.command
            .as_ref()
            .map(|command| {
                ctx.command_status(
                    command.target.unwrap_or(CommandTarget::From(ctx.node_id())),
                    &command.invocation,
                )
            })
            .transpose()
    }

    /// Sync the label text widget to the current label.
    fn sync_label(&self, ctx: &mut dyn Context) -> Result<()> {
        let box_id = ctx.get_or_create_slot::<BoxSlot>(|| {
            Border::new()
                .with_glyphs(self.glyphs)
                .with_border_style(roles::BUTTON_BORDER)
                .with_fill()
        })?;
        let center_id = ctx.get_or_create_slot_of::<CenterSlot>(box_id, Center::new)?;
        let label_id = ctx.get_or_create_slot_of::<LabelSlot>(center_id, || {
            Text::new(self.label.clone()).with_style(roles::BUTTON_LABEL)
        })?;
        ctx.with_widget_mut(label_id, |text: &mut Text, _| {
            text.set_text(self.label.clone());
            Ok(())
        })?;
        ctx.set_layout_of(label_id, Layout::column().max_width(self.label_width()))?;
        Ok(())
    }
}

impl Widget for Button {
    fn semantics(&self, ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("button".into()),
            label: Some(self.label.clone()),
            action_status: self.command_status(ctx)?,
            ..WidgetSemantics::default()
        })
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
        self.sync_label(ctx)
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer(roles::BUTTON);
        if self.active {
            rndr.push_layer(WidgetState::Pressed.layer());
        }
        if ctx.is_on_focus_path() {
            rndr.push_layer(WidgetState::Focused.layer());
        }
        if matches!(self.command_status(ctx)?, Some(CommandStatus::Disabled(_))) {
            rndr.push_layer(WidgetState::Disabled.layer());
        }
        Ok(())
    }

    fn on_event(&mut self, event: &Event, ctx: &mut dyn Context) -> Result<EventOutcome> {
        if let Event::Mouse(mouse_event) = event
            && self.handle_click(ctx, *mouse_event)?
        {
            return Ok(EventOutcome::Handle);
        }
        Ok(EventOutcome::Ignore)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("button")
    }
}

#[cfg(test)]
mod tests {
    use canopy::{Canopy, Loader, ViewContextExt, style::Color, testing::harness::Harness};

    use super::*;

    struct ActionOwner;

    #[derive_commands]
    impl ActionOwner {
        fn eligibility(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
            Ok(CommandStatus::Disabled("Unavailable".into()))
        }

        #[command(enabled = "eligibility")]
        fn activate(&self) {}
    }

    impl Widget for ActionOwner {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let mut button = Button::new("Save").with_command(
                Self::cmd_activate()
                    .call()
                    .with_target(CommandTarget::Exact(ctx.node_id())),
            );
            button.set_active(true);
            ctx.add_child(button)?;
            Ok(())
        }
    }

    impl Loader for ActionOwner {
        fn load(canopy: &mut Canopy) -> Result<()> {
            canopy.add_commands::<Self>()
        }
    }

    #[test]
    fn semantic_eligibility_is_independent_of_active_state() -> Result<()> {
        let mut harness = Harness::builder(ActionOwner).size(20, 4).build()?;
        let button = harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<Button>())?
            .expect("button mounted");
        harness.canopy.with_context(button, |ctx| {
            ctx.with_widget_mut(button, |button: &mut Button, ctx| {
                let active = button.semantics(ctx)?;
                button.set_active(false);
                let inactive = button.semantics(ctx)?;
                assert_eq!(active, inactive);
                assert_eq!(active.label.as_deref(), Some("Save"));
                assert_eq!(active.selected, None);
                assert_eq!(
                    active.action_status,
                    Some(CommandStatus::Disabled("Unavailable".into()))
                );
                Ok(())
            })
        })
    }

    /// Scene with an independently removable action target.
    struct ActionScene;

    impl Widget for ActionScene {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let owner = ctx.add_child(ActionOwner)?;
            ctx.add_child(
                Button::new("External action").with_command(
                    ActionOwner::cmd_activate()
                        .call()
                        .with_target(CommandTarget::Exact(owner.into())),
                ),
            )?;
            Ok(())
        }
    }

    impl Loader for ActionScene {
        fn load(canopy: &mut Canopy) -> Result<()> {
            canopy.add_commands::<ActionOwner>()
        }
    }

    #[test]
    fn detached_buttons_and_removed_targets_still_publish() -> Result<()> {
        let mut harness = Harness::builder(ActionScene).size(20, 4).build()?;
        let (owner, button) = harness.with_root_context(|_: &mut ActionScene, ctx| {
            let children = ctx.children();
            Ok((children[0], children[1]))
        })?;
        harness.canopy.with_root_context(|ctx| ctx.detach(button))?;
        harness.render()?;
        let snapshot = harness
            .canopy
            .snapshot()
            .expect("published detached button");
        let detached = snapshot
            .nodes
            .iter()
            .find(|node| node.id == button)
            .expect("button retained");
        assert!(!detached.attached);
        assert!(matches!(
            detached.semantics.action_status,
            Some(CommandStatus::Disabled(_))
        ));
        harness.canopy.with_root_context(|ctx| {
            ctx.attach(ctx.root_id(), button)?;
            ctx.remove_subtree(owner)
        })?;
        harness.render()?;
        let snapshot = harness.canopy.snapshot().expect("published stale target");
        let live = snapshot
            .nodes
            .iter()
            .find(|node| node.id == button)
            .expect("button retained");
        assert!(live.attached);
        assert!(matches!(
            live.semantics.action_status,
            Some(CommandStatus::Disabled(_))
        ));
        Ok(())
    }

    #[test]
    fn button_label_role_survives_an_extra_center() -> Result<()> {
        let mut harness = Harness::builder(ActionOwner).size(20, 4).build()?;
        harness
            .canopy
            .style_mut()
            .rules()
            .fg("button/active/text", Color::Red)
            .apply();
        harness.render()?;
        let label_style = |harness: &Harness| {
            harness
                .canopy
                .snapshot()
                .expect("published button")
                .cells
                .iter()
                .find(|cell| cell.ch == 'S')
                .expect("Save label")
                .style
        };
        let before = label_style(&harness);
        harness.with_root_context(|_: &mut ActionOwner, ctx| {
            ctx.with_unique_descendant::<Button, _>(|_, ctx| {
                let border = ctx.get_slot::<BoxSlot>()?.expect("button border");
                let center = ctx
                    .get_slot_of::<CenterSlot>(border)?
                    .expect("label center");
                let label = ctx.get_slot_of::<LabelSlot>(center)?.expect("button label");
                ctx.edit_structure(&mut |ctx| {
                    let wrapper = ctx.create_detached(Center::new())?;
                    ctx.detach(label.into())?;
                    ctx.attach(wrapper.into(), label.into())?;
                    ctx.attach(center.into(), wrapper.into())
                })
            })
        })?;
        harness.render()?;
        assert_eq!(label_style(&harness), before);
        Ok(())
    }
}
