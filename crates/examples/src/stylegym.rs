//! Stylegym: A demonstration app for Canopy's styling features.
//!
//! This example showcases themes, effects, and modal overlays in a two-pane layout.

use canopy::{
    Binder, Canopy, Context, Loader, ViewContext, command, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::{Direction, Edges, Layout},
    render::Render,
    style::{StyleMap, dracula, effects, gruvbox, solarized},
    widget::Widget,
    widgets::{Dropdown, DropdownItem, Modal, Root, Selector, SelectorItem, frame},
};

/// Theme option for the dropdown.
#[derive(Clone)]
pub struct ThemeOption {
    /// Theme display name.
    pub name: &'static str,
    /// Function to build the theme's StyleMap.
    pub builder: fn() -> StyleMap,
}

impl DropdownItem for ThemeOption {
    fn label(&self) -> &str {
        self.name
    }
}

/// Effect option for the selector.
#[derive(Clone)]
pub struct EffectOption {
    /// Effect display name.
    pub name: &'static str,
}

impl SelectorItem for EffectOption {
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
        EffectOption { name: "Dim" },
        EffectOption { name: "Brighten" },
        EffectOption { name: "Grayscale" },
        EffectOption { name: "Invert" },
        EffectOption { name: "Hue Shift" },
        EffectOption { name: "Bold" },
        EffectOption { name: "Italic" },
    ]
}

/// Key for the controls frame.
const KEY_CONTROLS: &str = "controls";
/// Key for the theme frame.
const KEY_THEME_FRAME: &str = "theme_frame";
/// Key for the theme dropdown.
const KEY_THEME_DROPDOWN: &str = "theme_dropdown";
/// Key for the effects frame.
const KEY_EFFECTS_FRAME: &str = "effects_frame";
/// Key for the effects selector.
const KEY_EFFECTS_SELECTOR: &str = "effects_selector";
/// Key for the right container stack.
const KEY_RIGHT_CONTAINER: &str = "right_container";
/// Key for the demo frame.
const KEY_DEMO_FRAME: &str = "demo_frame";
/// Key for the modal overlay.
const KEY_MODAL: &str = "modal";

/// The demo content pane showing styled samples.
pub struct DemoContent;

#[derive_commands]
impl DemoContent {}

impl Widget for DemoContent {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background with root style so effects apply to empty space
        rndr.fill("", rect, ' ')?;

        let mut row = 0;

        // Color palette section
        rndr.text("frame/title", rect.line(row), "Color Palette")?;
        row += 1;

        if rect.h > row + 8 {
            rndr.text("red", rect.line(row), "████ Red")?;
            rndr.text("orange", rect.line(row + 1), "████ Orange")?;
            rndr.text("yellow", rect.line(row + 2), "████ Yellow")?;
            rndr.text("green", rect.line(row + 3), "████ Green")?;
            rndr.text("cyan", rect.line(row + 4), "████ Cyan")?;
            rndr.text("blue", rect.line(row + 5), "████ Blue")?;
            rndr.text("violet", rect.line(row + 6), "████ Violet")?;
            rndr.text("magenta", rect.line(row + 7), "████ Magenta")?;
            row += 9;
        }

        // Text styles section
        if rect.h > row + 5 {
            rndr.text("frame/title", rect.line(row), "Text Styles")?;
            row += 1;
            rndr.text("", rect.line(row), "Normal text sample")?;
            row += 1;
            rndr.text("text/bold", rect.line(row), "Bold text sample")?;
            row += 1;
            rndr.text("text/italic", rect.line(row), "Italic text sample")?;
            row += 1;
            rndr.text("text/underline", rect.line(row), "Underlined text sample")?;
            row += 2;
        }

        // Instructions section
        if rect.h > row + 4 {
            rndr.text("frame/title", rect.line(row), "Controls")?;
            row += 1;
            rndr.text("", rect.line(row), "Tab: cycle focus")?;
            row += 1;
            rndr.text("", rect.line(row), "Space/Enter: toggle selection")?;
            row += 1;
            rndr.text("", rect.line(row), "m: show modal, Esc: hide modal")?;
        }

        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

/// Modal content widget.
struct ModalContent;

#[derive_commands]
impl ModalContent {}

impl Widget for ModalContent {
    fn render(&mut self, rndr: &mut Render, ctx: &dyn ViewContext) -> Result<()> {
        let view = ctx.view();
        let rect = view.view_rect_local();

        // Fill background so dimmed content doesn't show through
        rndr.fill("", rect, ' ')?;

        rndr.text("", rect.line(0), "This is a modal overlay.")?;
        rndr.text("", rect.line(1), "Press Esc to dismiss.")?;

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
    /// Available themes.
    themes: Vec<ThemeOption>,
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
            themes: available_themes(),
        }
    }

    /// Execute a closure with the right container widget.
    fn with_right_container<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut Container, &mut dyn Context) -> Result<R>,
    {
        c.with_keyed(KEY_RIGHT_CONTAINER, f)
    }

    /// Execute a closure with the demo frame widget.
    fn with_demo_frame<F, R>(&self, c: &mut dyn Context, f: F) -> Result<R>
    where
        F: FnOnce(&mut frame::Frame, &mut dyn Context) -> Result<R>,
    {
        self.with_right_container(c, |_, ctx| ctx.with_keyed(KEY_DEMO_FRAME, f))
    }

    /// Show the modal overlay.
    #[command]
    pub fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.modal_visible {
            return Ok(());
        }
        self.modal_visible = true;

        self.with_right_container(c, |_, ctx| {
            if ctx.child_keyed(KEY_MODAL).is_some() {
                return Ok(());
            }
            let modal_id = ctx.add_child_keyed(KEY_MODAL, Modal::new())?;
            let frame_id =
                ctx.add_child_to(modal_id, frame::Frame::new().with_title("Demo Modal"))?;
            ctx.add_child_to(frame_id, ModalContent)?;

            ctx.with_layout_of(frame_id, &mut |layout| {
                layout.min_width = Some(35);
                layout.max_width = Some(40);
                layout.min_height = Some(5);
                layout.max_height = Some(7);
            })?;
            Ok(())
        })?;

        // Dim the demo content frame
        self.with_demo_frame(c, |_frame, ctx| {
            ctx.push_effect(ctx.node_id(), effects::dim(0.5))
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
            if let Some(modal_id) = ctx.child_keyed(KEY_MODAL) {
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

        if selected_idx != self.current_theme && selected_idx < self.themes.len() {
            self.current_theme = selected_idx;
            let theme_builder = self.themes[selected_idx].builder;
            let new_style = theme_builder();
            c.set_style(new_style);
        }
        Ok(())
    }

    /// Apply the selected effects from the selector to the demo pane.
    #[command]
    pub fn apply_effects(&mut self, c: &mut dyn Context) -> Result<()> {
        let selected_indices = c
            .try_with_unique_descendant::<Selector<EffectOption>, _>(|selector, _ctx| {
                Ok(selector.selected_indices().to_vec())
            })?
            .unwrap_or_default();

        self.with_demo_frame(c, |_frame, ctx| {
            // Clear all existing effects on demo pane
            ctx.clear_effects(ctx.node_id())?;

            // Apply effects in selection order
            let effect_list = available_effects();
            for idx in selected_indices {
                if let Some(effect_option) = effect_list.get(idx) {
                    let effect = match effect_option.name {
                        "Dim" => effects::dim(0.5),
                        "Brighten" => effects::brighten(1.5),
                        "Grayscale" => effects::saturation(0.0),
                        "Invert" => effects::invert_rgb(),
                        "Hue Shift" => effects::hue_shift(180.0),
                        "Bold" => effects::bold(),
                        "Italic" => effects::italic(),
                        _ => continue,
                    };
                    ctx.push_effect(ctx.node_id(), effect)?;
                }
            }
            Ok(())
        })?;
        Ok(())
    }
}

/// A simple container widget that just renders its children.
struct Container;

#[derive_commands]
impl Container {}

impl Widget for Container {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::fill()
    }
}

impl Widget for Stylegym {
    fn render(&mut self, _r: &mut Render, _ctx: &dyn ViewContext) -> Result<()> {
        Ok(())
    }

    fn layout(&self) -> Layout {
        Layout::row().flex_horizontal(1).flex_vertical(1)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        // Create left frame (controls) - preserve Frame's padding for border
        let left_frame_id =
            c.add_child_keyed(KEY_CONTROLS, frame::Frame::new().with_title("Controls"))?;
        c.with_layout_of(left_frame_id, &mut |layout| {
            *layout = Layout::column()
                .fixed_width(32)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;

        // Create theme dropdown with its own frame - no fixed height so it can expand
        let theme_frame_id = c.add_child_to_keyed(
            left_frame_id,
            KEY_THEME_FRAME,
            frame::Frame::new().with_title("Theme"),
        )?;
        c.add_child_to_keyed(
            theme_frame_id,
            KEY_THEME_DROPDOWN,
            Dropdown::new(available_themes()),
        )?;
        c.with_layout_of(theme_frame_id, &mut |layout| {
            *layout = Layout::column().flex_horizontal(1).padding(Edges::all(1));
        })?;

        // Create effects selector with its own frame
        let effects_frame_id = c.add_child_to_keyed(
            left_frame_id,
            KEY_EFFECTS_FRAME,
            frame::Frame::new().with_title("Effects"),
        )?;
        c.add_child_to_keyed(
            effects_frame_id,
            KEY_EFFECTS_SELECTOR,
            Selector::new(available_effects()),
        )?;
        c.with_layout_of(effects_frame_id, &mut |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;

        // Create right container with Stack layout for modal overlay
        let right_container_id = c.add_child_keyed(KEY_RIGHT_CONTAINER, Container)?;
        c.with_layout_of(right_container_id, &mut |layout| {
            *layout = Layout::fill().direction(Direction::Stack);
        })?;

        // Create right frame (demo content)
        let right_frame_id = c.add_child_to_keyed(
            right_container_id,
            KEY_DEMO_FRAME,
            frame::Frame::new().with_title("Demo"),
        )?;
        c.add_child_to(right_frame_id, DemoContent)?;
        c.with_layout_of(right_frame_id, &mut |layout| {
            *layout = Layout::fill().padding(Edges::all(1));
        })?;

        Ok(())
    }
}

impl Loader for Stylegym {
    fn load(c: &mut Canopy) {
        c.add_commands::<Self>();
        c.add_commands::<DemoContent>();
        c.add_commands::<ModalContent>();
        c.add_commands::<Container>();
        c.add_commands::<Dropdown<ThemeOption>>();
        c.add_commands::<Selector<EffectOption>>();
    }
}

/// Set up key bindings for the stylegym demo.
pub fn setup_bindings(cnpy: &mut Canopy) -> Result<()> {
    Binder::new(cnpy)
        .defaults::<Root>()
        .with_path("stylegym/")
        .key('q', "root::quit()")
        .key(key::KeyCode::Tab, "root::focus_next()")
        .key(key::KeyCode::BackTab, "root::focus_prev()")
        .key('m', "stylegym::show_modal()")
        .key(key::KeyCode::Esc, "stylegym::hide_modal()")
        // Global j/k for navigation between focusable items
        .key('j', "root::focus_next()")
        .key('k', "root::focus_prev()")
        .with_path("dropdown")
        .key(
            key::KeyCode::Enter,
            "dropdown::confirm(); stylegym::apply_theme()",
        )
        .key(' ', "dropdown::toggle()")
        .key(key::KeyCode::Down, "dropdown::select_next()")
        .key(key::KeyCode::Up, "dropdown::select_prev()")
        // Mouse click on dropdown triggers theme application
        .mouse(
            mouse::Button::Left + mouse::Action::Down,
            "stylegym::apply_theme()",
        )
        .with_path("selector")
        .key(' ', "selector::toggle(); stylegym::apply_effects()")
        .key(
            key::KeyCode::Enter,
            "selector::toggle(); stylegym::apply_effects()",
        )
        .key(key::KeyCode::Down, "selector::select_next()")
        .key(key::KeyCode::Up, "selector::select_prev()")
        // Mouse click on selector triggers effect application
        .mouse(
            mouse::Button::Left + mouse::Action::Down,
            "stylegym::apply_effects()",
        );
    Ok(())
}
