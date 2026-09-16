//! Button widget.

use std::{borrow::Cow, ops::Range};

use canopy::{
    Canopy, Context, ContextExt, Loader, NodeName, Render, ViewContext, Widget, WidgetSemantics,
    commands::{CommandAction, CommandCall, CommandStatus, CommandTarget},
    derive_commands,
    error::Result,
    geom::{Line, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    style::{WidgetState, roles},
    text,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    Border, Center,
    boxed::{BoxGlyphs, SINGLE},
};

canopy::slot!(LabelSlot: ButtonLabel);
canopy::slot!(BoxSlot: Border);
canopy::slot!(CenterSlot: Center);

/// Default activation bindings exposed through `button.default_bindings()`.
///
/// The path reaches a button wherever it is mounted, and matches the label and
/// the border too, so a click anywhere on the button resolves to the button
/// that contains it. The bindings are ordinary application records, so an
/// application can rebind or unbind them like any other.
const DEFAULT_BINDINGS: &str = r#"
canopy.keymap({
    path = "**/button/**/",
    {
        key = { "Enter", "Space" },
        mouse = "LeftDown",
        description = "Activate the button",
        action = command.button.press(),
    },
})
"#;

/// Button widget that runs a command when it is activated.
///
/// Activation is the `button::press` command, which a click, `Enter`, or
/// `Space` reaches through ordinary bindings. Install them with
/// [`Loader::load`] and `button.default_bindings()`, or bind `press` however an
/// application prefers. A modal that admits only its own framework group must
/// bind activation in that group.
///
/// User activation of a disabled action is consumed without dispatching.
/// Calling [`Button::press`] directly still reports command errors.
pub struct Button {
    /// Button label.
    label: String,
    /// Label character a key is expected to reach this button by.
    accelerator: Option<char>,
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
            accelerator: None,
            command: None,
            glyphs: SINGLE,
            active: false,
        }
    }

    /// Mark the label character that a key reaches this button by.
    ///
    /// The first matching character takes the [`roles::BUTTON_KEY`] style, so
    /// the label names its key without repeating it and keeps its spelling. An
    /// ASCII letter matches without case; any other character must match
    /// exactly. A label with no match is left alone.
    ///
    /// This declares what the button shows. Binding the key stays with whoever
    /// owns the binding, so the mnemonic and its binding are written together
    /// and a button cannot install a key of its own.
    #[must_use]
    pub fn with_accelerator(mut self, accelerator: char) -> Self {
        self.accelerator = Some(accelerator);
        self
    }

    /// Build a button with a specified glyph set.
    pub fn with_glyphs(mut self, glyphs: BoxGlyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Build a button that dispatches a command when clicked.
    #[must_use]
    pub fn with_command(mut self, command: CommandCall) -> Self {
        self.set_command(command);
        self
    }

    /// Set the command this button runs, replacing any earlier one.
    ///
    /// A composed dialog builds its buttons before a host knows what each
    /// answer does, so the action arrives after construction.
    pub fn set_command(&mut self, command: CommandCall) {
        self.command = Some(command.action());
    }

    /// Set whether the button is active.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Trigger the button action.
    #[command(enabled = "press_status")]
    pub fn press(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(command) = self.command.as_ref() else {
            return Ok(());
        };
        // A click puts focus on the button first. The action can close a modal,
        // remove the button, or move focus itself, so focusing afterwards would
        // undo what it did. A direct or keyboard call has no pointer to follow
        // and leaves focus alone.
        if ctx.current_mouse_event().is_some() {
            ctx.set_focus(ctx.node_id())?;
        }
        ctx.dispatch(
            command.target.unwrap_or(CommandTarget::From(ctx.node_id())),
            &command.invocation,
        )?;
        Ok(())
    }

    /// Report the configured action's eligibility as this command's own.
    ///
    /// Discovery describes `press`, not the action behind it, so a disabled
    /// action must disable the command that runs it. A button with no action
    /// presses as a no-op and stays enabled.
    fn press_status(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(self.command_status(ctx)?.unwrap_or(CommandStatus::Enabled))
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
        let label_id = ctx.get_or_create_slot_of::<LabelSlot>(center_id, ButtonLabel::default)?;
        let label = self.label.clone();
        let accelerator = self.accelerator;
        ctx.with_widget_mut(label_id, |text: &mut ButtonLabel, _| {
            text.set_label(label, accelerator);
            Ok(())
        })?;
        ctx.set_layout_of(label_id, Layout::column().max_width(self.label_width()))?;
        Ok(())
    }
}

impl Loader for Button {
    fn load(canopy: &mut Canopy) -> Result<()> {
        canopy.add_commands::<Self>()?;
        canopy.register_default_bindings("button", DEFAULT_BINDINGS)
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

    /// Take focus only when there is something to activate.
    ///
    /// A decorative button stays out of keyboard traversal. One whose action is
    /// disabled keeps focus, so its reason stays reachable.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.command.is_some()
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

    fn name(&self) -> NodeName {
        NodeName::convert("button")
    }
}

/// The label inside a button, with at most one highlighted character.
///
/// [`Text`](crate::Text) paints one style across a line, and a mnemonic needs
/// two. This stays private, so the button keeps its border and centring
/// composition and no other widget inherits a one-off renderer.
#[derive(Default)]
pub struct ButtonLabel {
    /// Text on the button.
    label: String,
    /// Byte range of the highlighted grapheme, when the label has one.
    accelerator: Option<Range<usize>>,
}

impl ButtonLabel {
    /// Show `label`, highlighting the first character `accelerator` names.
    fn set_label(&mut self, label: String, accelerator: Option<char>) {
        self.accelerator = accelerator.and_then(|key| accelerator_range(&label, key));
        self.label = label;
    }
}

impl Widget for ButtonLabel {
    fn layout(&self) -> Layout {
        Layout::fill()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // One row, as wide as the label. A narrower offer clips rather than
        // wraps, because a button is a fixed shape around one line.
        c.clamp(Size::new(
            u32::try_from(text::display_width(&self.label)).unwrap_or(u32::MAX),
            1,
        ))
    }

    fn render(&mut self, render: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        let budget = area.w as usize;
        let shown = text::truncate_end(&self.label, budget);
        let line = area.line(0)?;
        render.text(roles::BUTTON_LABEL, line, &shown)?;

        let Some(range) = self.accelerator.clone() else {
            return Ok(());
        };
        // A clipped label spends its last column on the marker, so only the
        // columns before it still spell the original characters.
        let kept =
            text::display_width(&shown).saturating_sub(usize::from(matches!(shown, Cow::Owned(_))));
        let column = text::display_width(&self.label[..range.start]);
        let width = text::display_width(&self.label[range.clone()]);
        if column.saturating_add(width) > kept {
            return Ok(());
        }
        render.text(
            roles::BUTTON_KEY,
            Line::new(
                line.tl
                    .x
                    .saturating_add(u32::try_from(column).unwrap_or(u32::MAX)),
                line.tl.y,
                u32::try_from(width).unwrap_or(u32::MAX),
            ),
            &self.label[range],
        )
    }

    fn name(&self) -> NodeName {
        NodeName::convert("button_label")
    }
}

/// Return the byte range of the first grapheme in `label` that `accelerator`
/// names.
///
/// The range is a whole grapheme cluster, so a highlighted letter keeps any
/// mark that belongs to it rather than being split from it.
fn accelerator_range(label: &str, accelerator: char) -> Option<Range<usize>> {
    label.grapheme_indices(true).find_map(|(offset, grapheme)| {
        names_grapheme(grapheme, accelerator).then(|| offset..offset + grapheme.len())
    })
}

/// Return whether `accelerator` names `grapheme`.
///
/// An ASCII letter matches without case, because a mnemonic is written as one
/// letter and the label keeps whichever case it is spelled in. Anything else
/// matches exactly, so case folding never changes what a non-ASCII label means.
fn names_grapheme(grapheme: &str, accelerator: char) -> bool {
    let Some(first) = grapheme.chars().next() else {
        return false;
    };
    if accelerator.is_ascii_alphabetic() {
        first.eq_ignore_ascii_case(&accelerator)
    } else {
        grapheme.chars().eq([accelerator])
    }
}

#[cfg(test)]
mod tests {
    use canopy::{
        Canopy, FocusDirection, FocusScope, FrameworkBindingGroup, InputSpec, Loader,
        ModalBindings, ModalOptions, NodeId, ViewContextExt,
        commands::CommandError,
        error::Error,
        event::{key, key::Key, mouse, mouse::Mouse},
        geom::PointI32,
        layout::Direction,
        style::Color,
        testing::harness::Harness,
    };

    use super::*;
    use crate::Container;

    /// Root that owns an action and shows it as a button.
    #[derive(Default)]
    struct ActionOwner {
        /// Whether the action reports itself eligible.
        enabled: bool,
        /// Whether the action fails once invoked.
        fail: bool,
        /// Times the action ran, eligible or not.
        activations: usize,
    }

    #[derive_commands]
    impl ActionOwner {
        fn eligibility(&self, _ctx: &dyn ViewContext) -> Result<CommandStatus> {
            Ok(if self.enabled {
                CommandStatus::Enabled
            } else {
                CommandStatus::Disabled("Unavailable".into())
            })
        }

        #[command(enabled = "eligibility")]
        fn activate(&mut self) -> Result<()> {
            self.activations += 1;
            if self.fail {
                Err(Error::Invalid("action failed".into()))
            } else {
                Ok(())
            }
        }
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
            Button::load(canopy)?;
            canopy.add_commands::<Self>()
        }
    }

    /// Build a harness whose application installed the button defaults.
    ///
    /// Loading registers the script; an application runs it, and so does a
    /// test, before any configuration that might replace a binding.
    fn activating<W: Widget + Loader + 'static>(
        root: W,
        width: u32,
        height: u32,
    ) -> Result<Harness> {
        let mut harness = Harness::builder(root)
            .bindings("button-defaults", "button.default_bindings()")
            .size(width, height)
            .build()?;
        harness.render()?;
        Ok(harness)
    }

    /// Return the screen origin of `node`.
    fn origin(harness: &Harness, node: NodeId) -> PointI32 {
        harness
            .canopy
            .with_root_view(|ctx| ctx.view_of(node).expect("live node").outer.tl)
    }

    /// Build a left-button press at `location`.
    fn press_at(location: PointI32) -> mouse::MouseEvent {
        mouse::MouseEvent {
            action: mouse::Action::Down,
            button: mouse::Button::Left,
            modifiers: key::Empty,
            location,
        }
    }

    /// Return the only button in the tree.
    fn the_button(harness: &Harness) -> NodeId {
        harness
            .canopy
            .with_root_view(|ctx| ctx.unique_descendant::<Button>())
            .expect("button lookup")
            .expect("button mounted")
            .into()
    }

    #[test]
    fn semantic_eligibility_is_independent_of_active_state() -> Result<()> {
        let mut harness = activating(ActionOwner::default(), 20, 4)?;
        let button = the_button(&harness);
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
            let owner = ctx.add_child(ActionOwner::default())?;
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
            ActionOwner::load(canopy)
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
        let mut harness = activating(ActionOwner::default(), 20, 4)?;
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

    #[test]
    fn a_click_anywhere_on_the_button_activates_it() -> Result<()> {
        let mut harness = activating(
            ActionOwner {
                enabled: true,
                ..ActionOwner::default()
            },
            20,
            5,
        )?;
        let button = the_button(&harness);
        let label = harness
            .find_nodes("**/button/**/button_label")?
            .first()
            .copied()
            .expect("button label");

        // The border and the label are separate nodes, so a click on either
        // must still reach the button that contains them.
        assert_ne!(origin(&harness, button), origin(&harness, label));
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.mouse(press_at(origin(&harness, label)))?;
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 2));
        Ok(())
    }

    /// Two buttons that run the same action with different arguments.
    #[derive(Default)]
    struct Tally {
        /// Tag of each button press, in order.
        pressed: Vec<i64>,
    }

    #[derive_commands]
    impl Tally {
        /// Record one press.
        /// @param tag Identifies the button that ran this command.
        #[command]
        fn note(&mut self, tag: i64) {
            self.pressed.push(tag);
        }
    }

    impl Widget for Tally {
        fn layout(&self) -> Layout {
            Layout::fill().direction(Direction::Column)
        }

        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let owner = ctx.node_id();
            for tag in [1, 2] {
                ctx.add_child(
                    Button::new(format!("Button {tag}")).with_command(
                        Self::call_note(tag).with_target(CommandTarget::Exact(owner)),
                    ),
                )?;
            }
            Ok(())
        }

        fn name(&self) -> NodeName {
            NodeName::convert("tally")
        }
    }

    impl Loader for Tally {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Button::load(canopy)?;
            canopy.add_commands::<Self>()
        }
    }

    #[test]
    fn a_click_activates_the_button_it_landed_on() -> Result<()> {
        let mut harness = activating(Tally::default(), 20, 8)?;
        let buttons = harness.find_nodes("**/button")?;
        assert_eq!(buttons.len(), 2, "both buttons mounted");
        for button in buttons.iter().rev() {
            harness.mouse(press_at(origin(&harness, *button)))?;
        }
        harness.with_root_widget(|tally: &mut Tally| {
            assert_eq!(tally.pressed, [2, 1], "each click ran its own button");
        });
        Ok(())
    }

    #[test]
    fn only_an_unmodified_left_press_activates() -> Result<()> {
        let mut harness = activating(
            ActionOwner {
                enabled: true,
                ..ActionOwner::default()
            },
            20,
            5,
        )?;
        let location = origin(&harness, the_button(&harness));
        for event in [
            mouse::MouseEvent {
                modifiers: key::Ctrl,
                ..press_at(location)
            },
            mouse::MouseEvent {
                action: mouse::Action::Up,
                ..press_at(location)
            },
            mouse::MouseEvent {
                button: mouse::Button::Right,
                ..press_at(location)
            },
            mouse::MouseEvent {
                action: mouse::Action::Moved,
                button: mouse::Button::None,
                ..press_at(location)
            },
        ] {
            harness.mouse(event)?;
        }
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 0, "only a plain left press activates");
        });
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 1));
        Ok(())
    }

    #[test]
    fn disabled_activation_is_inert_and_rechecked_each_time() -> Result<()> {
        let mut harness = activating(ActionOwner::default(), 20, 5)?;
        let location = origin(&harness, the_button(&harness));
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 0);
            owner.enabled = true;
        });
        // Eligibility can change after the frame was rendered, so the click
        // reads it again rather than trusting what was painted.
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 1);
            owner.enabled = false;
        });
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 1));

        // A direct call still reports the reason rather than doing nothing.
        let button = the_button(&harness);
        harness.canopy.with_context(button, |ctx| {
            ctx.with_widget_mut(button, |button: &mut Button, ctx| {
                assert!(matches!(
                    button.press(ctx),
                    Err(Error::Command(CommandError::Disabled { .. }))
                ));
                Ok(())
            })
        })?;
        Ok(())
    }

    #[test]
    fn activation_propagates_action_errors() -> Result<()> {
        let mut harness = activating(
            ActionOwner {
                enabled: true,
                fail: true,
                ..ActionOwner::default()
            },
            20,
            5,
        )?;
        let location = origin(&harness, the_button(&harness));
        assert!(harness.mouse(press_at(location)).is_err());
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 1));
        Ok(())
    }

    #[test]
    fn a_click_focuses_the_button_and_the_keyboard_activates_it() -> Result<()> {
        let mut harness = activating(
            ActionOwner {
                enabled: true,
                ..ActionOwner::default()
            },
            20,
            5,
        )?;
        let button = the_button(&harness);
        harness.mouse(press_at(origin(&harness, button)))?;
        assert_eq!(
            harness.canopy.with_root_view(|ctx| ctx.focused_node()),
            Some(button),
            "a click leaves focus on the button it activated"
        );
        harness.key(key::KeyCode::Enter)?;
        harness.key(' ')?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 3, "Enter and Space activate the focus");
        });
        Ok(())
    }

    /// Root holding one button with no action at all.
    struct Decorative;

    impl Widget for Decorative {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            ctx.add_child(Button::new("Label only"))?;
            Ok(())
        }
    }

    impl Loader for Decorative {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Button::load(canopy)
        }
    }

    #[test]
    fn a_button_without_an_action_stays_out_of_traversal_and_presses_as_a_no_op() -> Result<()> {
        let mut harness = activating(Decorative, 20, 5)?;
        let button = the_button(&harness);
        harness
            .canopy
            .with_root_context(|ctx| ctx.focus_move(FocusScope::Root, FocusDirection::Next))?;
        assert_ne!(
            harness.canopy.with_root_view(|ctx| ctx.focused_node()),
            Some(button),
            "a decorative button is not a focus stop"
        );
        // Pressing it anyway does nothing and reports nothing.
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.canopy.with_context(button, |ctx| {
            ctx.with_widget_mut(button, |button: &mut Button, ctx| button.press(ctx))
        })
    }

    /// Root whose action removes the button that ran it.
    #[derive(Default)]
    struct SelfRemoving {
        /// The button, once mounted.
        button: Option<NodeId>,
    }

    #[derive_commands]
    impl SelfRemoving {
        /// Remove the button that ran this command.
        #[command]
        fn dismiss(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let Some(button) = self.button.take() else {
                return Ok(());
            };
            // The button is borrowed while its own command runs, so removal
            // waits for the dispatch that asked for it to return.
            ctx.remove_after_dispatch(button)?;
            Ok(())
        }
    }

    impl Widget for SelfRemoving {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let owner = ctx.node_id();
            self.button = Some(
                ctx.add_child(
                    Button::new("Dismiss").with_command(
                        Self::cmd_dismiss()
                            .call()
                            .with_target(CommandTarget::Exact(owner)),
                    ),
                )?
                .into(),
            );
            Ok(())
        }
    }

    impl Loader for SelfRemoving {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Button::load(canopy)?;
            canopy.add_commands::<Self>()
        }
    }

    #[test]
    fn an_action_may_remove_the_button_that_ran_it() -> Result<()> {
        let mut harness = activating(SelfRemoving::default(), 20, 5)?;
        let button = the_button(&harness);
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.render()?;
        assert!(
            harness.find_nodes("**/button")?.is_empty(),
            "the action removed its own button"
        );
        Ok(())
    }

    #[test]
    fn an_accelerator_names_the_first_matching_grapheme() {
        // An ASCII letter matches without case and keeps the label's spelling.
        assert_eq!(accelerator_range("Save", 's'), Some(0..1));
        assert_eq!(accelerator_range("Save", 'S'), Some(0..1));
        assert_eq!(accelerator_range("Save", 'v'), Some(2..3));
        // The first match wins, so a repeated letter highlights once.
        assert_eq!(accelerator_range("Rename", 'e'), Some(1..2));
        // A missing match leaves the label alone.
        assert_eq!(accelerator_range("Save", 'z'), None);
        assert_eq!(accelerator_range("", 'a'), None);

        // A non-ASCII character matches exactly, so case folding never changes
        // what a label means.
        assert_eq!(accelerator_range("Ärger", 'Ä'), Some(0..2));
        assert_eq!(accelerator_range("Ärger", 'ä'), None);

        // A highlighted letter keeps the mark that belongs to it, rather than
        // being split from its cluster.
        let combining = "cafe\u{0301}";
        assert_eq!(accelerator_range(combining, 'e'), Some(3..6));
    }

    /// Root holding one button with an accelerator.
    struct Mnemonic(&'static str, char);

    impl Widget for Mnemonic {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            ctx.add_child(Button::new(self.0).with_accelerator(self.1))?;
            Ok(())
        }
    }

    impl Loader for Mnemonic {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Button::load(canopy)
        }
    }

    #[test]
    fn the_accelerator_takes_its_own_style_without_changing_the_label() -> Result<()> {
        for (label, key, shown, marked, plain) in [
            ("Save", 's', "Save", 'S', 'a'),
            // A wide grapheme before the key shifts it by two columns, not one.
            // The buffer dump fills a wide cell's second column, so only the
            // narrow tail is compared as text.
            ("\u{754c}ave", 'v', "ave", 'v', 'a'),
            ("Rename", 'e', "Rename", 'e', 'R'),
        ] {
            let harness = activating(Mnemonic(label, key), 20, 5)?;
            let screen = harness.tbuf().lines().join("\n");
            assert!(
                screen.contains(shown),
                "the label keeps its spelling, got {screen}"
            );
            let snapshot = harness.canopy.snapshot().expect("published button");
            let style_of = |needle: char| {
                snapshot
                    .cells
                    .iter()
                    .find(|cell| cell.ch == needle)
                    .map(|cell| (cell.style.fg, cell.style.attrs))
                    .unwrap_or_else(|| panic!("{needle:?} renders in {label:?}"))
            };
            assert_ne!(
                style_of(marked),
                style_of(plain),
                "the key stands out from the rest of {label:?}"
            );
        }

        // A label with no match renders as one style throughout.
        let harness = activating(Mnemonic("Save", 'z'), 20, 5)?;
        let snapshot = harness.canopy.snapshot().expect("published button");
        let styles = "Save"
            .chars()
            .map(|needle| {
                snapshot
                    .cells
                    .iter()
                    .find(|cell| cell.ch == needle)
                    .map(|cell| (cell.style.fg, cell.style.attrs))
                    .expect("the label renders")
            })
            .collect::<Vec<_>>();
        assert!(
            styles.windows(2).all(|pair| pair[0] == pair[1]),
            "an unmatched accelerator leaves the label alone"
        );
        Ok(())
    }

    #[test]
    fn a_clipped_label_drops_an_accelerator_it_cannot_show() -> Result<()> {
        // Six columns leave room for the border, one column of padding on each
        // side, and two label columns, so the marker takes the second and the
        // key at column three is gone.
        let harness = activating(Mnemonic("Rename", 'm'), 6, 5)?;
        let screen = harness.tbuf().lines().join("\n");
        assert!(
            screen.contains('\u{2026}'),
            "the label is marked as clipped"
        );
        assert!(
            !screen.contains("Rename"),
            "the label does not overrun its button, got {screen}"
        );
        let snapshot = harness.canopy.snapshot().expect("published button");
        assert!(
            !snapshot.cells.iter().any(|cell| cell.ch == 'm'),
            "a key clipped away is not painted somewhere else"
        );
        Ok(())
    }

    /// Framework group admitted while the guarded dialog is open.
    const GUARDED: FrameworkBindingGroup = FrameworkBindingGroup::new("button.test_dialog");

    /// Root with a dialog it opens as an exclusive modal.
    #[derive(Default)]
    struct Guarded {
        /// Times the dialog's button ran its action.
        activations: usize,
        /// The dialog subtree, once mounted.
        dialog: Option<NodeId>,
        /// The dialog's button, once mounted.
        button: Option<NodeId>,
    }

    #[derive_commands]
    impl Guarded {
        /// Accept the question the dialog asks.
        #[command]
        fn accept(&mut self) {
            self.activations += 1;
        }
    }

    impl Widget for Guarded {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let owner = ctx.node_id();
            let dialog = ctx.add_child(Container::column().with_name("dialog"))?;
            let button = ctx.add_child_to(
                dialog,
                Button::new("Accept")
                    .with_command(Self::call_accept().with_target(CommandTarget::Exact(owner))),
            )?;
            self.dialog = Some(dialog.into());
            self.button = Some(button.into());
            Ok(())
        }
    }

    impl Loader for Guarded {
        fn load(canopy: &mut Canopy) -> Result<()> {
            Button::load(canopy)?;
            canopy.add_commands::<Self>()?;
            // The dialog owns activation inside its own group, because an
            // exclusive scope admits nothing else. The records are the same
            // three inputs, on a path of the dialog's own.
            for (input, description) in [
                (InputSpec::Key(Key::parse_spec("Enter")?), "Activate"),
                (InputSpec::Key(Key::parse_spec("Space")?), "Activate"),
                (InputSpec::Mouse(Mouse::parse_spec("LeftDown")?), "Activate"),
            ] {
                canopy.bind_framework(
                    GUARDED,
                    input,
                    canopy::BindingOptions {
                        path: Some("**/dialog/**/".parse()?),
                        scope: canopy::BindingScope::Exclusive(GUARDED),
                        description: description.to_string(),
                        source: None,
                        phase: canopy::BindingPhase::AfterWidget,
                    },
                    Button::call_press(),
                )?;
            }
            Ok(())
        }
    }

    #[test]
    fn an_exclusive_modal_admits_only_its_own_activation_bindings() -> Result<()> {
        let mut harness = activating(Guarded::default(), 20, 6)?;
        let (owner, dialog, button) = harness.with_root_context(|guarded: &mut Guarded, ctx| {
            Ok((
                ctx.node_id(),
                guarded.dialog.expect("dialog mounted"),
                guarded.button.expect("button mounted"),
            ))
        })?;

        // Outside the modal the ordinary defaults carry the click.
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.with_root_widget(|guarded: &mut Guarded| assert_eq!(guarded.activations, 1));

        harness.canopy.with_root_context(|ctx| {
            ctx.open_modal(ModalOptions {
                owner,
                modal: dialog,
                initial_focus: button,
                dim_target: None,
                bindings: ModalBindings::Framework(GUARDED),
            })?;
            Ok(())
        })?;
        harness.render()?;

        // The group admits its own records and nothing else, so the same three
        // inputs still activate while unrelated defaults stay out.
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.key(key::KeyCode::Enter)?;
        harness.key(' ')?;
        harness.with_root_widget(|guarded: &mut Guarded| {
            assert_eq!(guarded.activations, 4, "the dialog's own bindings activate");
        });
        assert_eq!(
            harness
                .canopy
                .available_bindings(None)?
                .bindings
                .iter()
                .filter(|binding| binding.path_filter == "**/button/**/")
                .count(),
            0,
            "the application defaults are not admitted through the modal"
        );
        Ok(())
    }

    #[test]
    fn the_defaults_are_ordinary_records_an_application_can_replace() -> Result<()> {
        let mut harness = activating(
            ActionOwner {
                enabled: true,
                ..ActionOwner::default()
            },
            20,
            5,
        )?;
        let location = origin(&harness, the_button(&harness));

        // Loading again installs no second record and leaves one winner.
        harness.script(
            r#"
            button.default_bindings()
            local activation = 0
            for _, binding in canopy.bindings() do
                if binding.path == "**/button/**/" then
                    activation += 1
                end
            end
            canopy.assert(activation == 3, "one record per activation input, got " .. activation)
            "#,
        )?;

        harness.script(r#"canopy.unbind_key("Enter", { path = "**/button/**/" })"#)?;
        harness.key(key::KeyCode::Enter)?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 0, "an unbound key no longer activates");
        });

        // Rebinding the same selector replaces the default outright.
        harness.script(
            r#"canopy.bind_mouse("LeftDown", {
                path = "**/button/**/",
                description = "Ignore the click",
            }, function() end)"#,
        )?;
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 0, "the override took the click");
        });

        // Loading again is idempotent and installs nothing, so a replaced
        // binding stays replaced.
        Button::load(&mut harness.canopy).expect_err("loading after finalization is refused");
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(
                owner.activations, 0,
                "loading does not reinstall the default"
            );
        });
        Ok(())
    }
}
