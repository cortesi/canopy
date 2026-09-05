//! Stylegym: A demonstration app for Canopy's styling features.
//!
//! This example showcases themes, effects, and modal overlays in a two-pane
//! layout.

use canopy::{
    CanopyBuilder, command, derive_commands,
    layout::Edges,
    prelude::*,
    style::{StyleMap, dracula, effects, effects::Effect, gruvbox, solarized},
};
use canopy_widgets::{Center, Dropdown, Frame, Label, Root, Selector};

/// Default bindings for the style gym demo.
const DEFAULT_BINDINGS: &str = r#"
root.default_bindings()

canopy.bind_command("q", { phase = "before_widget", path = "stylegym/", description = "Quit" }, "root::quit")
canopy.bind_command("Tab", { phase = "before_widget", path = "stylegym/", description = "Next focus" }, "root::focus", "Next")
canopy.bind_command("BackTab", { phase = "before_widget", path = "stylegym/", description = "Previous focus" }, "root::focus", "Prev")
canopy.bind_command("m", { phase = "before_widget", path = "stylegym/", description = "Show modal" }, "stylegym::show_modal")
canopy.bind_command("Esc", { phase = "before_widget", path = "stylegym/", description = "Hide modal" }, "stylegym::hide_modal")
canopy.bind_command("j", { phase = "before_widget", path = "stylegym/", description = "Next focus" }, "root::focus", "Next")
canopy.bind_command("k", { phase = "before_widget", path = "stylegym/", description = "Previous focus" }, "root::focus", "Prev")

canopy.bind("Enter", { phase = "before_widget", path = "dropdown", description = "Apply theme" }, function()
    dropdown.confirm()
    stylegym.apply_theme()
end)
canopy.bind_command("Space", { phase = "before_widget", path = "dropdown", description = "Toggle dropdown" }, "dropdown::toggle")
canopy.bind_command("Down", { phase = "before_widget", path = "dropdown", description = "Next option" }, "dropdown::select_by", 1)
canopy.bind_command("Up", { phase = "before_widget", path = "dropdown", description = "Previous option" }, "dropdown::select_by", -1)
canopy.bind_mouse("LeftDown", { phase = "after_widget", path = "dropdown", description = "Apply theme" }, function()
    stylegym.apply_theme()
end)

canopy.bind("Space", { phase = "before_widget", path = "selector", description = "Toggle effect" }, function()
    selector.toggle()
    stylegym.apply_effects()
end)
canopy.bind("Enter", { phase = "before_widget", path = "selector", description = "Toggle effect" }, function()
    selector.toggle()
    stylegym.apply_effects()
end)
canopy.bind_command("Down", { phase = "before_widget", path = "selector", description = "Next effect" }, "selector::select_by", 1)
canopy.bind_command("Up", { phase = "before_widget", path = "selector", description = "Previous effect" }, "selector::select_by", -1)
canopy.bind_mouse("LeftDown", { phase = "after_widget", path = "selector", description = "Apply effects" }, function()
    stylegym.apply_effects()
end)
"#;

/// Theme option for the dropdown.
#[derive(Clone)]
pub struct ThemeOption {
    /// Theme display name.
    pub name: &'static str,
    /// Function to build the theme's StyleMap.
    pub builder: fn() -> StyleMap,
}

impl Label for ThemeOption {
    fn label(&self) -> &str {
        self.name
    }
}

/// Effect option for the selector.
#[derive(Clone)]
pub struct EffectOption {
    /// Effect display name.
    pub name: &'static str,
    /// Style effect applied when this option is selected.
    pub effect: Effect,
}

impl Label for EffectOption {
    fn label(&self) -> &str {
        self.name
    }
}

/// Available themes.
fn available_themes() -> Vec<ThemeOption> {
    vec![
        ThemeOption {
            name: "Solarized Dark",
            builder: solarized::solarized_dark,
        },
        ThemeOption {
            name: "Solarized Light",
            builder: solarized::solarized_light,
        },
        ThemeOption {
            name: "Gruvbox Dark",
            builder: gruvbox::gruvbox_dark,
        },
        ThemeOption {
            name: "Dracula",
            builder: dracula::dracula,
        },
    ]
}

/// Available effects.
fn available_effects() -> Vec<EffectOption> {
    vec![
        EffectOption {
            name: "Dim",
            effect: effects::brightness(0.5),
        },
        EffectOption {
            name: "Brighten",
            effect: effects::brightness(1.5),
        },
        EffectOption {
            name: "Grayscale",
            effect: effects::saturation(0.0),
        },
        EffectOption {
            name: "Invert",
            effect: effects::invert_rgb(),
        },
        EffectOption {
            name: "Hue Shift",
            effect: effects::hue_shift(180.0),
        },
        EffectOption {
            name: "Bold",
            effect: effects::bold(),
        },
        EffectOption {
            name: "Italic",
            effect: effects::italic(),
        },
    ]
}

// Typed keys for keyed children
canopy::key!(ControlsSlot: Frame);
canopy::key!(ThemeFrameSlot: Frame);
canopy::key!(ThemeDropdownSlot: Dropdown<ThemeOption>);
canopy::key!(EffectsFrameSlot: Frame);
canopy::key!(EffectsSelectorSlot: Selector<EffectOption>);
canopy::key!(RightContainerSlot: Container);
canopy::key!(DemoFrameSlot: Frame);
canopy::key!(DemoContentSlot: DemoContent);
canopy::key!(ModalSlot: Center);

/// The demo content pane showing styled samples.
pub struct DemoContent;

impl Widget for DemoContent {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background with root style so effects apply to empty space
        rndr.fill("", rect, ' ')?;

        let mut row = 0;

        // Color palette section
        rndr.text("frame/title", rect.line(row)?, "Color Palette")?;
        row += 1;

        if rect.h > row + 8 {
            rndr.text("red", rect.line(row)?, "████ Red")?;
            rndr.text("orange", rect.line(row + 1)?, "████ Orange")?;
            rndr.text("yellow", rect.line(row + 2)?, "████ Yellow")?;
            rndr.text("green", rect.line(row + 3)?, "████ Green")?;
            rndr.text("cyan", rect.line(row + 4)?, "████ Cyan")?;
            rndr.text("blue", rect.line(row + 5)?, "████ Blue")?;
            rndr.text("violet", rect.line(row + 6)?, "████ Violet")?;
            rndr.text("magenta", rect.line(row + 7)?, "████ Magenta")?;
            row += 9;
        }

        // Text styles section
        if rect.h > row + 5 {
            rndr.text("frame/title", rect.line(row)?, "Text Styles")?;
            row += 1;
            rndr.text("", rect.line(row)?, "Normal text sample")?;
            row += 1;
            rndr.text("text/bold", rect.line(row)?, "Bold text sample")?;
            row += 1;
            rndr.text("text/italic", rect.line(row)?, "Italic text sample")?;
            row += 1;
            rndr.text("text/underline", rect.line(row)?, "Underlined text sample")?;
            row += 2;
        }

        // Instructions section
        if rect.h > row + 4 {
            rndr.text("frame/title", rect.line(row)?, "Controls")?;
            row += 1;
            rndr.text("", rect.line(row)?, "Tab: cycle focus")?;
            row += 1;
            rndr.text("", rect.line(row)?, "Space/Enter: toggle selection")?;
            row += 1;
            rndr.text("", rect.line(row)?, "m: show modal, Esc: hide modal")?;
        }

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// Modal content widget.
struct ModalContent;

impl Widget for ModalContent {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background so dimmed content doesn't show through
        rndr.fill("", rect, ' ')?;

        rndr.text("", rect.line(0)?, "This is a modal overlay.")?;
        rndr.text("", rect.line(1)?, "Press Esc to dismiss.")?;

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// Root widget for the stylegym demo.
pub struct Stylegym {
    /// Whether the modal is currently shown.
    modal_visible: bool,
    /// Current theme index.
    current_theme: usize,
}

impl Default for Stylegym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl Stylegym {
    /// Create a new stylegym instance.
    pub fn new() -> Self {
        Self {
            modal_visible: false,
            current_theme: 0,
        }
    }

    /// Execute a closure with the right container widget.
    fn with_right_container<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Container, &mut dyn Context) -> Result<R>,
    {
        c.with_child::<RightContainerSlot, _>(f)
    }

    /// Execute a closure with the demo content widget.
    fn with_demo_content<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut DemoContent, &mut dyn Context) -> Result<R>,
    {
        self.with_right_container(c, |_, ctx| {
            ctx.with_child::<DemoFrameSlot, _>(|_, ctx| ctx.with_child::<DemoContentSlot, _>(f))
        })
    }

    /// Show the modal overlay.
    #[command]
    pub fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.modal_visible {
            return Ok(());
        }
        self.modal_visible = true;

        self.with_right_container(c, |_, ctx| {
            if ctx.has_child::<ModalSlot>()? {
                return Ok(());
            }
            let modal_id = ctx.add_keyed::<ModalSlot>(Center::new())?;
            let frame_id = ctx.add_child_to(modal_id, Frame::new().with_title("Demo Modal"))?;
            ctx.add_child_to(frame_id, ModalContent)?;

            let mut layout = Layout::fill().padding(Edges::all(1));
            layout.min_width = Some(35);
            layout.max_width = Some(40);
            layout.min_height = Some(5);
            layout.max_height = Some(7);
            ctx.set_layout_of(frame_id, layout)?;
            Ok(())
        })?;

        // Dim the demo content
        self.with_demo_content(c, |_content, ctx| {
            ctx.push_effect(ctx.node_id(), effects::brightness(0.5))
        })?;

        Ok(())
    }

    /// Hide the modal overlay.
    #[command]
    pub fn hide_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.modal_visible {
            return Ok(());
        }
        self.modal_visible = false;

        self.with_right_container(c, |_, ctx| {
            if let Some(modal_id) = ctx.child_keyed(ModalSlot::KEY) {
                ctx.remove_subtree(modal_id)?;
            }
            Ok(())
        })?;

        // Re-apply user effects (clears dim, applies selected effects)
        self.apply_effects(c)?;

        Ok(())
    }

    /// Apply the selected theme from the dropdown.
    #[command]
    pub fn apply_theme(&mut self, c: &mut dyn Context) -> Result<()> {
        let Some(selected_idx) =
            c.try_with_unique_descendant::<Dropdown<ThemeOption>, _>(|dropdown, _ctx| {
                Ok(dropdown.selected_index())
            })?
        else {
            return Ok(());
        };

        if selected_idx != self.current_theme {
            self.current_theme = selected_idx;
            let builder =
                c.try_with_unique_descendant::<Dropdown<ThemeOption>, _>(|dropdown, _ctx| {
                    Ok(dropdown.selected().builder)
                })?;
            if let Some(builder) = builder {
                c.set_style(builder());
            }
        }
        Ok(())
    }

    /// Apply the selected effects from the selector to the demo pane.
    #[command]
    pub fn apply_effects(&mut self, c: &mut dyn Context) -> Result<()> {
        let selected = c
            .try_with_unique_descendant::<Selector<EffectOption>, _>(|selector, _ctx| {
                Ok(selector
                    .selected_items()
                    .into_iter()
                    .map(|option| option.effect.clone())
                    .collect::<Vec<_>>())
            })?
            .unwrap_or_default();

        self.with_demo_content(c, |_content, ctx| {
            ctx.clear_effects(ctx.node_id())?;
            for effect in selected {
                ctx.push_effect(ctx.node_id(), effect)?;
            }
            if self.modal_visible {
                ctx.push_effect(ctx.node_id(), effects::brightness(0.5))?;
            }
            Ok(())
        })?;
        Ok(())
    }
}

/// A simple container widget that just renders its children.
struct Container;

impl Widget for Container {
    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

impl Widget for Stylegym {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Row)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        // Create left frame (controls) - preserve Frame's padding for border
        let left_frame_id = c.add_keyed::<ControlsSlot>(Frame::new().with_title("Controls"))?;
        c.set_layout_of(
            left_frame_id,
            Layout::column()
                .fixed_width(32)
                .flex_vertical(1)
                .padding(Edges::all(1)),
        )?;

        // Create theme dropdown with its own frame - no fixed height so it can
        // expand
        let theme_frame_id = c.add_keyed_to(
            left_frame_id,
            ThemeFrameSlot::KEY,
            Frame::new().with_title("Theme"),
        )?;
        c.add_keyed_to(
            theme_frame_id,
            ThemeDropdownSlot::KEY,
            Dropdown::new(available_themes()),
        )?;
        c.set_layout_of(
            theme_frame_id,
            Layout::column().flex_horizontal(1).padding(Edges::all(1)),
        )?;

        // Create effects selector with its own frame
        let effects_frame_id = c.add_keyed_to(
            left_frame_id,
            EffectsFrameSlot::KEY,
            Frame::new().with_title("Effects"),
        )?;
        c.add_keyed_to(
            effects_frame_id,
            EffectsSelectorSlot::KEY,
            Selector::new(available_effects()),
        )?;
        c.set_layout_of(effects_frame_id, Layout::fill().padding(Edges::all(1)))?;

        // Create right container with Stack layout for modal overlay
        let right_container_id = c.add_keyed::<RightContainerSlot>(Container)?;
        c.set_layout_of(
            right_container_id,
            Layout::fill().direction(Direction::Stack),
        )?;

        // Create right frame (demo content)
        let right_frame_id = c.add_keyed_to(
            right_container_id,
            DemoFrameSlot::KEY,
            Frame::new().with_title("Demo"),
        )?;
        c.add_keyed_to(right_frame_id, DemoContentSlot::KEY, DemoContent)?;
        c.set_layout_of(right_frame_id, Layout::fill().padding(Edges::all(1)))?;

        Ok(())
    }
}

impl Loader for Stylegym {
    fn load(c: &mut Canopy) -> Result<()> {
        Root::load(c)?;
        c.add_commands::<Self>()?;
        c.add_commands::<Dropdown<ThemeOption>>()?;
        c.add_commands::<Selector<EffectOption>>()?;
        Ok(())
    }
}

/// Set up key bindings for the stylegym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(DEFAULT_BINDINGS)
}

/// Queue this demo's bindings and native configuration in their builder phases.
#[must_use]
pub fn binding_setup(builder: CanopyBuilder) -> CanopyBuilder {
    builder.bindings("stylegym", DEFAULT_BINDINGS)
}
