//! Stylegym: A demonstration app for Canopy's styling features.
//!
//! This example showcases themes, effects, and modal overlays in a two-pane layout.

use std::any::Any;

use canopy::{
    Binder, Canopy, Context, Loader, NodeId, ViewContext, command, derive_commands,
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
    /// Left frame (controls) node ID.
    left_frame_id: Option<NodeId>,
    /// Right content container (Stack for modal overlay).
    right_container_id: Option<NodeId>,
    /// Right frame (demo content) node ID.
    right_frame_id: Option<NodeId>,
    /// Theme dropdown frame node ID.
    theme_frame_id: Option<NodeId>,
    /// Theme dropdown node ID.
    theme_dropdown_id: Option<NodeId>,
    /// Effects selector frame node ID.
    effects_frame_id: Option<NodeId>,
    /// Effects selector node ID.
    effects_selector_id: Option<NodeId>,
    /// Modal node ID (when visible).
    modal_id: Option<NodeId>,
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
            left_frame_id: None,
            right_container_id: None,
            right_frame_id: None,
            theme_frame_id: None,
            theme_dropdown_id: None,
            effects_frame_id: None,
            effects_selector_id: None,
            modal_id: None,
            modal_visible: false,
            current_theme: 0,
            themes: available_themes(),
        }
    }

    /// Show the modal overlay.
    #[command]
    pub fn show_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if self.modal_visible {
            return Ok(());
        }
        self.modal_visible = true;

        // Create modal if needed
        if self.modal_id.is_none() {
            let modal_id = c.create_detached(Modal::new());
            let frame_id =
                c.add_child_to(modal_id, frame::Frame::new().with_title("Demo Modal"))?;
            c.add_child_to(frame_id, ModalContent)?;

            c.with_layout_of(frame_id, &mut |layout| {
                layout.min_width = Some(35);
                layout.max_width = Some(40);
                layout.min_height = Some(5);
                layout.max_height = Some(7);
            })?;

            self.modal_id = Some(modal_id);
        }

        // Dim the demo content frame
        if let Some(right_id) = self.right_frame_id {
            c.push_effect(right_id, effects::dim(0.5))?;
        }

        // Add modal to the right container (which has Stack direction)
        self.sync_right_container(c)?;

        Ok(())
    }

    /// Hide the modal overlay.
    #[command]
    pub fn hide_modal(&mut self, c: &mut dyn Context) -> Result<()> {
        if !self.modal_visible {
            return Ok(());
        }
        self.modal_visible = false;

        // Update right container to remove modal
        self.sync_right_container(c)?;

        // Re-apply user effects (clears dim, applies selected effects)
        self.apply_effects(c)?;

        Ok(())
    }

    /// Apply the selected theme from the dropdown.
    #[command]
    pub fn apply_theme(&mut self, c: &mut dyn Context) -> Result<()> {
        if let Some(dropdown_id) = self.theme_dropdown_id {
            let mut selected_idx = 0;
            c.with_widget_mut(dropdown_id, &mut |widget, _ctx| {
                let any = widget as &mut dyn Any;
                if let Some(dropdown) = any.downcast_mut::<Dropdown<ThemeOption>>() {
                    selected_idx = dropdown.selected_index();
                }
                Ok(())
            })?;

            if selected_idx != self.current_theme && selected_idx < self.themes.len() {
                self.current_theme = selected_idx;
                let theme_builder = self.themes[selected_idx].builder;
                let new_style = theme_builder();
                c.set_style(new_style);
            }
        }
        Ok(())
    }

    /// Apply the selected effects from the selector to the demo pane.
    #[command]
    pub fn apply_effects(&mut self, c: &mut dyn Context) -> Result<()> {
        let right_id = match self.right_frame_id {
            Some(id) => id,
            None => return Ok(()),
        };

        // Clear all existing effects on demo pane
        c.clear_effects(right_id)?;

        // Get selected effect indices from the selector
        if let Some(selector_id) = self.effects_selector_id {
            let mut selected_indices: Vec<usize> = Vec::new();
            c.with_widget_mut(selector_id, &mut |widget, _ctx| {
                let any = widget as &mut dyn Any;
                if let Some(selector) = any.downcast_mut::<Selector<EffectOption>>() {
                    selected_indices = selector.selected_indices().to_vec();
                }
                Ok(())
            })?;

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
                    c.push_effect(right_id, effect)?;
                }
            }
        }
        Ok(())
    }

    /// Synchronize the right container's children based on modal state.
    fn sync_right_container(&self, c: &mut dyn Context) -> Result<()> {
        let right_container_id = self
            .right_container_id
            .expect("right container not initialized");
        let right_frame_id = self.right_frame_id.expect("right frame not initialized");

        let mut children = vec![right_frame_id];

        if self.modal_visible
            && let Some(modal_id) = self.modal_id
        {
            children.push(modal_id);
        }

        c.set_children_of(right_container_id, children)?;
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
        let left_frame_id = c.create_detached(frame::Frame::new().with_title("Controls"));
        c.with_layout_of(left_frame_id, &mut |layout| {
            *layout = Layout::column()
                .fixed_width(32)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;

        // Create theme dropdown with its own frame - no fixed height so it can expand
        let theme_frame_id = c.create_detached(frame::Frame::new().with_title("Theme"));
        let theme_dropdown_id =
            c.add_child_to(theme_frame_id, Dropdown::new(available_themes()))?;
        c.with_layout_of(theme_frame_id, &mut |layout| {
            *layout = Layout::column().flex_horizontal(1).padding(Edges::all(1));
        })?;

        // Create effects selector with its own frame
        let effects_frame_id = c.create_detached(frame::Frame::new().with_title("Effects"));
        let effects_selector_id =
            c.add_child_to(effects_frame_id, Selector::new(available_effects()))?;
        c.with_layout_of(effects_frame_id, &mut |layout| {
            *layout = Layout::column()
                .flex_horizontal(1)
                .flex_vertical(1)
                .padding(Edges::all(1));
        })?;

        // Mount theme and effects frames to left frame
        c.set_children_of(left_frame_id, vec![theme_frame_id, effects_frame_id])?;

        // Create right container with Stack layout for modal overlay
        let right_container_id = c.create_detached(Container);
        c.with_layout_of(right_container_id, &mut |layout| {
            *layout = Layout::fill().direction(Direction::Stack);
        })?;

        // Create right frame (demo content)
        let right_frame_id = c.create_detached(frame::Frame::new().with_title("Demo"));
        let _demo_content_id = c.add_child_to(right_frame_id, DemoContent)?;
        c.with_layout_of(right_frame_id, &mut |layout| {
            *layout = Layout::fill().padding(Edges::all(1));
        })?;

        // Mount right frame to right container
        c.set_children_of(right_container_id, vec![right_frame_id])?;

        // Set up main children: left frame and right container
        c.set_children(vec![left_frame_id, right_container_id])?;

        self.left_frame_id = Some(left_frame_id);
        self.right_container_id = Some(right_container_id);
        self.right_frame_id = Some(right_frame_id);
        self.theme_frame_id = Some(theme_frame_id);
        self.theme_dropdown_id = Some(theme_dropdown_id);
        self.effects_frame_id = Some(effects_frame_id);
        self.effects_selector_id = Some(effects_selector_id);

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
