//! Button widget.

use std::{borrow::Cow, ops::Range};

use canopy::{
    Context, NodeName, Register, Setup, ViewContext, Widget,
    commands::{CommandCall, CommandStatus},
    derive_commands,
    error::Result,
    geom::{Line, Rect, Size},
    layout::{Layout, MeasureConstraints, Measurement},
    render::Render,
    runtime::WidgetSemantics,
    style::{WidgetState, roles},
    text,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::border::BoxGlyphs;

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

/// Columns between the label and each side of a button.
const PADDING: u32 = 2;

/// How a button draws.
///
/// A terminal cell has one ground, and a box drawing line runs through the
/// middle of its cell, so a border cannot enclose a fill of another color
/// without a ring of the ground showing inside it. The filled looks draw
/// their edges with block elements instead, which meet the edges of the
/// cells exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonLook {
    /// The label on a filled row, one row tall.
    #[default]
    Solid,
    /// A fill inset by half a cell on every side, three rows tall. Its edges
    /// fall on the middles of the outer cells, where a box border runs.
    Inset,
    /// A fill three rows tall, lit along its top edge and shaded along its
    /// bottom edge, as Textual draws its buttons.
    Bevel,
    /// The label inside a box border of these glyphs, on the ground of the
    /// view, three rows tall.
    Bordered(BoxGlyphs),
}

impl ButtonLook {
    /// Return the rows that a button of this look takes.
    pub const fn rows(self) -> u32 {
        match self {
            Self::Solid => 1,
            Self::Inset | Self::Bevel | Self::Bordered(_) => 3,
        }
    }

    /// Return the style prefix of the label and its key: a filled look draws
    /// them on its face.
    const fn label_prefix(self) -> &'static str {
        match self {
            Self::Solid | Self::Inset | Self::Bevel => "face/",
            Self::Bordered(_) => "",
        }
    }
}

/// Button widget that runs a command when it is activated.
///
/// Activation is the `button::press` command, which a click, `Enter`, or
/// `Space` reaches through ordinary bindings. Install them with
/// [`Register::register`] and `button.default_bindings()`, or bind `press`
/// however an application prefers. A modal that admits only its own framework
/// group must bind activation in that group.
///
/// The button posts its action with [`Context::post`], so the action runs once
/// the press has returned. It can then close a modal around the button, or
/// remove the button. User activation of a disabled action is consumed without
/// posting. Calling [`Button::press`] directly posts the action as well: the
/// action's errors surface when the outermost dispatch completes, not from the
/// call.
///
/// A button measures its label, and draws in its [`ButtonLook`]. It pushes the
/// `button` layer, then at most one state layer: `disabled`, `active`, or
/// `focused`, in that order of precedence. A filled look paints `face`, and its
/// label and key as `face/text` and `face/key`. A bevel adds
/// `face/highlight` and `face/shadow`, and an inset paints its edges as
/// `face/edge`, whose ground is the ground around the button. A bordered look
/// paints `border`, `fill`, `text`, and `key`.
pub struct Button {
    /// Button label.
    label: String,
    /// Label character a key is expected to reach this button by.
    accelerator: Option<char>,
    /// Command call to dispatch on click.
    command: Option<CommandCall>,
    /// How the button draws.
    look: ButtonLook,
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
            look: ButtonLook::default(),
            active: false,
        }
    }

    /// Mark the label character that a key reaches this button by.
    ///
    /// The first matching character takes the [`roles::KEY`] style, so
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

    /// Build a button that draws in `look`.
    #[must_use]
    pub fn with_look(mut self, look: ButtonLook) -> Self {
        self.look = look;
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
        self.command = Some(command);
    }

    /// Set whether the button is active.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Return the size that the button takes: its label with padding, in the
    /// rows of its look.
    pub fn size(&self) -> Size {
        Size::new(
            text::width(&self.label).saturating_add(PADDING * 2),
            self.look.rows(),
        )
    }

    /// Trigger the button action.
    #[command(enabled = "press_status")]
    pub fn press(&mut self, ctx: &mut dyn Context) -> Result<()> {
        let Some(command) = self.command.as_ref() else {
            return Ok(());
        };
        // A click puts focus on the button first. The action runs after the
        // press returns, and can move focus itself, so it keeps the last word.
        // A direct or keyboard call has no pointer to follow and leaves focus
        // alone.
        if ctx.current_mouse_event().is_some() {
            ctx.set_focus(ctx.node_id())?;
        }
        ctx.post(command)
    }

    /// Report the configured action's eligibility as this command's own.
    ///
    /// Discovery describes `press`, not the action behind it, so a disabled
    /// action must disable the command that runs it. A button with no action
    /// presses as a no-op and stays enabled.
    fn press_status(&self, ctx: &dyn ViewContext) -> Result<CommandStatus> {
        Ok(self.command_status(ctx)?.unwrap_or(CommandStatus::Enabled))
    }

    /// Read command eligibility without conflating it with the active state.
    fn command_status(&self, ctx: &dyn ViewContext) -> Result<Option<CommandStatus>> {
        if self.command.is_some() && !ctx.is_attached(ctx.node_id()) {
            return Ok(Some(CommandStatus::Disabled("Button is detached".into())));
        }
        self.command
            .as_ref()
            .map(|command| ctx.command_status(command))
            .transpose()
    }

    /// Return the state that the button shows, by precedence: a disabled
    /// action, then the active state, then focus.
    fn state(&self, ctx: &dyn ViewContext) -> Result<Option<WidgetState>> {
        Ok(
            if matches!(self.command_status(ctx)?, Some(CommandStatus::Disabled(_))) {
                Some(WidgetState::Disabled)
            } else if self.active {
                Some(WidgetState::Pressed)
            } else if ctx.is_on_focus_path(ctx.node_id()) {
                Some(WidgetState::Focused)
            } else {
                None
            },
        )
    }

    /// Draw the label on the middle row of `area`, centred between the
    /// padding, with its accelerator highlighted.
    fn draw_label(&self, render: &mut Render, area: Rect) -> Result<()> {
        let prefix = self.look.label_prefix();
        let room = area.w.saturating_sub(PADDING * 2).max(1).min(area.w);
        let shown = text::truncate_end(&self.label, room as usize);
        let width = text::width(&shown);
        let x = area.tl.x + area.w.saturating_sub(width) / 2;
        let y = area.tl.y + area.h / 2;
        render.text(
            &format!("{prefix}{}", roles::TEXT),
            Line::new(x, y, width),
            &shown,
        )?;
        let Some(range) = self
            .accelerator
            .and_then(|key| accelerator_range(&self.label, key))
        else {
            return Ok(());
        };
        // A clipped label spends its last column on the marker, so only the
        // columns before it still spell the original characters.
        let kept = width.saturating_sub(u32::from(matches!(shown, Cow::Owned(_))));
        let column = text::width(&self.label[..range.start]);
        let key_width = text::width(&self.label[range.clone()]);
        if column.saturating_add(key_width) > kept {
            return Ok(());
        }
        render.text(
            &format!("{prefix}{}", roles::KEY),
            Line::new(x.saturating_add(column), y, key_width),
            &self.label[range],
        )
    }

    /// Draw the inset edges: half and quarter blocks in the face color on
    /// the ground around the button.
    fn draw_inset(render: &mut Render, area: Rect) -> Result<()> {
        let right = area.tl.x + area.w - 1;
        let bottom = area.tl.y + area.h - 1;
        let span = area.w.saturating_sub(2);
        let row = |render: &mut Render, y: u32, left: char, middle: char, end: char| {
            render.text("face/edge", Line::new(area.tl.x, y, 1), &left.to_string())?;
            render.fill("face/edge", Rect::new(area.tl.x + 1, y, span, 1), middle)?;
            render.text("face/edge", Line::new(right, y, 1), &end.to_string())
        };
        if area.h >= 2 {
            row(render, area.tl.y, '▗', '▄', '▖')?;
            row(render, bottom, '▝', '▀', '▘')?;
        }
        let first = if area.h >= 2 {
            area.tl.y + 1
        } else {
            area.tl.y
        };
        let last = if area.h >= 2 { bottom } else { area.tl.y + 1 };
        for y in first..last {
            render.text("face/edge", Line::new(area.tl.x, y, 1), "▐")?;
            render.text("face/edge", Line::new(right, y, 1), "▌")?;
        }
        Ok(())
    }

    /// Draw a box border of `glyphs` around `area`.
    fn draw_border(render: &mut Render, area: Rect, glyphs: BoxGlyphs) -> Result<()> {
        let right = area.tl.x + area.w - 1;
        let bottom = area.tl.y + area.h - 1;
        let span = area.w.saturating_sub(2);
        let border = roles::BORDER;
        render.text(
            border,
            Line::new(area.tl.x, area.tl.y, 1),
            &glyphs.topleft.to_string(),
        )?;
        render.fill(
            border,
            Rect::new(area.tl.x + 1, area.tl.y, span, 1),
            glyphs.horizontal,
        )?;
        render.text(
            border,
            Line::new(right, area.tl.y, 1),
            &glyphs.topright.to_string(),
        )?;
        for y in area.tl.y + 1..bottom {
            render.text(
                border,
                Line::new(area.tl.x, y, 1),
                &glyphs.vertical.to_string(),
            )?;
            render.text(border, Line::new(right, y, 1), &glyphs.vertical.to_string())?;
        }
        render.text(
            border,
            Line::new(area.tl.x, bottom, 1),
            &glyphs.bottomleft.to_string(),
        )?;
        render.fill(
            border,
            Rect::new(area.tl.x + 1, bottom, span, 1),
            glyphs.horizontal,
        )?;
        render.text(
            border,
            Line::new(right, bottom, 1),
            &glyphs.bottomright.to_string(),
        )
    }
}

impl Register for Button {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()?;
        setup.register_default_bindings("button", DEFAULT_BINDINGS)
    }
}

impl Widget for Button {
    fn semantics(&self, ctx: &dyn ViewContext) -> Result<WidgetSemantics> {
        Ok(WidgetSemantics {
            role: Some("button".into()),
            label: Some(self.label.clone()),
            activation_status: self.command_status(ctx)?,
            ..WidgetSemantics::default()
        })
    }

    fn layout(&self) -> Layout {
        Layout::column()
    }

    fn measure(&self, c: MeasureConstraints) -> Measurement {
        // A button is a fixed shape around one line, so a narrower offer
        // clips the label rather than wrapping it.
        c.clamp(self.size())
    }

    /// Take focus only when there is something to activate.
    ///
    /// A decorative button stays out of keyboard traversal. One whose action is
    /// disabled keeps focus, so its reason stays reachable.
    fn accept_focus(&self, _ctx: &dyn ViewContext) -> bool {
        self.command.is_some()
    }

    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        rndr.push_layer("button");
        if let Some(state) = self.state(ctx)? {
            rndr.push_layer(state.layer());
        }
        let area = ctx.view().view_rect_local();
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        match self.look {
            ButtonLook::Solid => rndr.fill("face", area, ' ')?,
            ButtonLook::Inset => {
                rndr.fill("face", area, ' ')?;
                Self::draw_inset(rndr, area)?;
            }
            ButtonLook::Bevel => {
                rndr.fill("face", area, ' ')?;
                if area.h >= 3 {
                    let bottom = area.tl.y + area.h - 1;
                    rndr.fill(
                        "face/highlight",
                        Rect::new(area.tl.x, area.tl.y, area.w, 1),
                        '▔',
                    )?;
                    rndr.fill("face/shadow", Rect::new(area.tl.x, bottom, area.w, 1), '▁')?;
                }
            }
            ButtonLook::Bordered(glyphs) => {
                rndr.fill("fill", area, ' ')?;
                if area.h >= 3 && area.w >= 2 {
                    Self::draw_border(rndr, area, glyphs)?;
                }
            }
        }
        self.draw_label(rndr, area)
    }

    fn name(&self) -> NodeName {
        NodeName::convert("button")
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
        ContextExt, NodeId, Register, Setup, ViewContextExt,
        commands::{CommandError, CommandTarget},
        error::Error,
        geom::PointI32,
        input::{
            BindingAction, BindingOptions, BindingPhase, BindingTier, FrameworkBindingGroup,
            InputSpec, ModalBindings, ModalOptions, key, key::Key, mouse, mouse::Mouse,
        },
        layout::Direction,
        runtime::NoticeSource,
        style::Color,
        testing::harness::Harness,
        tree::{FocusDirection, FocusScope},
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
                Self::spec_activate()
                    .call()
                    .with_target(CommandTarget::Exact(ctx.node_id())),
            );
            button.set_active(true);
            ctx.add_child(ctx.node_id(), button)?;
            Ok(())
        }
    }

    impl Register for ActionOwner {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)?;
            setup.add_commands::<Self>()
        }
    }

    /// Build a harness whose application installed the button defaults.
    ///
    /// Registration adds the script; an application runs it, and so does a
    /// test, before any configuration that might replace a binding.
    fn activating<W: Widget + Register + 'static>(
        root: W,
        width: u32,
        height: u32,
    ) -> Result<Harness> {
        let mut harness = Harness::builder(root)
            .register::<W>()
            .script("button-defaults", "button.default_bindings()")
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
            .with_root_view(|ctx| ctx.unique_descendant::<Button>(ctx.node_id()))
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
                    active.activation_status,
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
            let owner = ctx.add_child(ctx.node_id(), ActionOwner::default())?;
            ctx.add_child(
                ctx.node_id(),
                Button::new("External action").with_command(
                    ActionOwner::spec_activate()
                        .call()
                        .with_target(CommandTarget::Exact(owner.into())),
                ),
            )?;
            Ok(())
        }
    }

    impl Register for ActionScene {
        fn register(setup: &mut Setup) -> Result<()> {
            ActionOwner::register(setup)
        }
    }

    #[test]
    fn detached_buttons_and_removed_targets_still_publish() -> Result<()> {
        let mut harness = Harness::builder(ActionScene)
            .register::<ActionScene>()
            .size(20, 4)
            .build()?;
        let (owner, button) = harness.with_root_widget_context(|_: &mut ActionScene, ctx| {
            let children = ctx.children_of(ctx.node_id());
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
            detached.semantics.activation_status,
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
            live.semantics.activation_status,
            Some(CommandStatus::Disabled(_))
        ));
        Ok(())
    }

    /// Return the rows of a button of `look`, drawn alone in its own size.
    fn drawn(look: ButtonLook) -> Result<Vec<String>> {
        let button = Button::new("Save").with_look(look);
        let size = button.size();
        let mut harness = Harness::builder(button)
            .register::<Button>()
            .size(size.w, size.h)
            .build()?;
        harness.render()?;
        Ok(harness.tbuf().lines())
    }

    #[test]
    fn each_look_draws_its_shape_around_the_centred_label() -> Result<()> {
        assert_eq!(drawn(ButtonLook::Solid)?, ["  Save  "]);
        assert_eq!(
            drawn(ButtonLook::Inset)?,
            ["▗▄▄▄▄▄▄▖", "▐ Save ▌", "▝▀▀▀▀▀▀▘"]
        );
        assert_eq!(
            drawn(ButtonLook::Bevel)?,
            ["▔▔▔▔▔▔▔▔", "  Save  ", "▁▁▁▁▁▁▁▁"]
        );
        assert_eq!(
            drawn(ButtonLook::Bordered(BoxGlyphs::ROUND))?,
            ["╭──────╮", "│ Save │", "╰──────╯"]
        );
        Ok(())
    }

    #[test]
    fn a_filled_button_paints_its_face_and_its_state() -> Result<()> {
        let mut harness = activating(ActionOwner::default(), 20, 4)?;
        harness
            .canopy
            .style_mut()
            .rules()
            .bg("button/disabled/face/text", Color::Red)
            .fg("button/disabled/face/text", Color::Blue)
            .apply();
        harness.render()?;
        let cell = |ch: char| {
            harness
                .canopy
                .snapshot()
                .expect("published button")
                .buffer
                .cells()
                .iter()
                .find(|cell| cell.ch == ch)
                .expect("label cell")
                .style
        };
        // The action is disabled, so the label takes the face of that state.
        assert_eq!(cell('S').fg, Color::Blue);
        assert_eq!(cell('S').bg, Color::Red);
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
        // The padding is the button's own, so a click beside the label and a
        // click on it both reach the button.
        let corner = origin(&harness, button);
        let label = PointI32 {
            x: corner.x + 3,
            y: corner.y,
        };
        harness.mouse(press_at(corner))?;
        harness.mouse(press_at(label))?;
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
                    ctx.node_id(),
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

    impl Register for Tally {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)?;
            setup.add_commands::<Self>()
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

        // A direct call posts the action, so the press itself succeeds, and
        // the reason arrives when the boundary around the call completes.
        let button = the_button(&harness);
        let result = harness.canopy.with_context(button, |ctx| {
            ctx.with_widget_mut(button, |button: &mut Button, ctx| {
                assert!(button.press(ctx).is_ok(), "the press only posts");
                Ok(())
            })
        });
        assert!(matches!(
            result,
            Err(Error::Command(CommandError::Disabled { .. }))
        ));
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 1));
        Ok(())
    }

    #[test]
    fn activation_reports_action_errors_as_notices() -> Result<()> {
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
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| assert_eq!(owner.activations, 1));
        let notice = harness
            .canopy
            .notices()
            .last()
            .expect("the failed activation is a notice");
        assert_eq!(notice.source, NoticeSource::Binding);
        Ok(())
    }

    /// Root whose button removes itself, reading the click that pressed it.
    #[derive(Default)]
    struct Discarding {
        /// The button, until its action removes it.
        button: Option<NodeId>,
        /// Actions of the clicks the action read.
        clicks: Vec<mouse::Action>,
    }

    #[derive_commands]
    impl Discarding {
        /// Remove the button that ran this action.
        #[command]
        fn discard(&mut self, ctx: &mut dyn Context, event: mouse::MouseEvent) -> Result<()> {
            self.clicks.push(event.action);
            if let Some(button) = self.button.take() {
                ctx.remove_subtree(button)?;
            }
            Ok(())
        }
    }

    impl Widget for Discarding {
        fn on_mount(&mut self, ctx: &mut dyn Context) -> Result<()> {
            let mut button = Button::new("Close").with_command(
                Self::spec_discard()
                    .call()
                    .with_target(CommandTarget::Exact(ctx.node_id())),
            );
            button.set_active(true);
            self.button = Some(ctx.add_child(ctx.node_id(), button)?.into());
            Ok(())
        }
    }

    impl Register for Discarding {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)?;
            setup.add_commands::<Self>()
        }
    }

    #[test]
    fn an_action_can_remove_its_button_and_read_its_click() -> Result<()> {
        let mut harness = activating(Discarding::default(), 20, 5)?;
        let button = the_button(&harness);
        harness.mouse(press_at(origin(&harness, button)))?;
        harness.with_root_widget(|root: &mut Discarding| {
            assert_eq!(
                root.clicks,
                [mouse::Action::Down],
                "the action read the click"
            );
            assert_eq!(root.button, None);
        });
        let gone = harness
            .canopy
            .with_root_view(|ctx| ctx.type_id_of(button).is_none());
        assert!(gone, "the button's own action removed it");
        assert!(
            harness.canopy.notices().is_empty(),
            "removal raised nothing"
        );
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
            ctx.add_child(ctx.node_id(), Button::new("Label only"))?;
            Ok(())
        }
    }

    impl Register for Decorative {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)
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
                    ctx.node_id(),
                    Button::new("Dismiss").with_command(
                        Self::spec_dismiss()
                            .call()
                            .with_target(CommandTarget::Exact(owner)),
                    ),
                )?
                .into(),
            );
            Ok(())
        }
    }

    impl Register for SelfRemoving {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)?;
            setup.add_commands::<Self>()
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
            ctx.add_child(ctx.node_id(), Button::new(self.0).with_accelerator(self.1))?;
            Ok(())
        }
    }

    impl Register for Mnemonic {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)
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
                    .buffer
                    .cells()
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
                    .buffer
                    .cells()
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
            !snapshot.buffer.cells().iter().any(|cell| cell.ch == 'm'),
            "a key clipped away is not painted somewhere else"
        );
        Ok(())
    }

    /// Framework group admitted while the guarded dialog is open.
    const GUARDED: FrameworkBindingGroup = FrameworkBindingGroup::new("button.test_dialog");

    /// Root with a dialog it opens as a framework-group modal.
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
            let dialog = ctx.add_child(ctx.node_id(), Container::column().with_name("dialog"))?;
            let button = ctx.add_child(
                dialog,
                Button::new("Accept")
                    .with_command(Self::call_accept().with_target(CommandTarget::Exact(owner))),
            )?;
            self.dialog = Some(dialog.into());
            self.button = Some(button.into());
            Ok(())
        }
    }

    impl Register for Guarded {
        fn register(setup: &mut Setup) -> Result<()> {
            Button::register(setup)?;
            setup.add_commands::<Self>()?;
            // The dialog owns activation inside its own group, because a
            // framework-group modal admits nothing else. The records are the
            // same three inputs, on a path of the dialog's own.
            for (input, description) in [
                (InputSpec::Key(Key::parse_spec("Enter")?), "Activate"),
                (InputSpec::Key(Key::parse_spec("Space")?), "Activate"),
                (InputSpec::Mouse(Mouse::parse_spec("LeftDown")?), "Activate"),
            ] {
                setup.bind(
                    input,
                    BindingOptions {
                        show_in_help: true,
                        path: Some("**/dialog/**/".parse()?),
                        tier: BindingTier::Framework(GUARDED),
                        description: description.to_string(),
                        source: None,
                        phase: Some(BindingPhase::AfterWidget),
                    },
                    BindingAction::Command(Button::call_press()),
                )?;
            }
            Ok(())
        }
    }

    #[test]
    fn a_framework_group_modal_admits_only_its_own_activation_bindings() -> Result<()> {
        let mut harness = activating(Guarded::default(), 20, 6)?;
        let (owner, dialog, button) =
            harness.with_root_widget_context(|guarded: &mut Guarded, ctx| {
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
                bindings: ModalBindings::Framework {
                    groups: &[GUARDED],
                    intents: &[],
                },
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
            r#"canopy.keymap({
                path = "**/button/**/",
                { mouse = "LeftDown", description = "Ignore the click", action = function() end },
            })"#,
        )?;
        harness.mouse(press_at(location))?;
        harness.with_root_widget(|owner: &mut ActionOwner| {
            assert_eq!(owner.activations, 0, "the override took the click");
        });
        Ok(())
    }
}
