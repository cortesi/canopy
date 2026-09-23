#![deny(unsafe_code)]
//! Minimal Canopy application, and the reference for building one outside the
//! Canopy workspace.
//!
//! The crate shows the whole external contract in one place: a widget with a
//! command, a [`CanopyBuilder`] pipeline, an optional trusted user
//! configuration directory, and a factory that every launch mode shares.

use canopy::{
    Canopy, CanopyBuilder, Context, ContextExt, NodeName, Register, Setup, ViewContext, Widget,
    derive_commands,
    error::Result,
    layout::{Align, Direction, Layout},
    render::Render,
    style::{StyleRules, themes::Palette},
};
use canopy_mcp::{ConfigHome, UserConfig};
use canopy_widgets::{KeyHint, Root, StatusBar, Text};

/// Default configuration: the startup script that runs when the user has no
/// `init.luau`, and the file a first run writes for them to edit.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.luau");

/// A greeting and a counter driven by one command.
pub struct Hello {
    /// Current counter value, rendered under the greeting.
    count: i32,
}

#[derive_commands]
impl Hello {
    /// Create the widget with a zeroed counter.
    fn new() -> Self {
        Self { count: 0 }
    }

    /// Add a signed amount to the counter.
    /// @param delta Negative values count down; positive values count up.
    #[command]
    pub fn bump(&mut self, delta: i32) {
        self.count = self.count.saturating_add(delta);
    }
}

impl Widget for Hello {
    fn layout(&self) -> Layout {
        // The footer bar keeps the last row, so align the greeting above it.
        Layout::fill()
            .direction(Direction::Column)
            .align_vertical(Align::End)
    }

    fn render(&mut self, render: &mut Render, context: &dyn ViewContext) -> Result<()> {
        render.push_layer("hello");
        let area = context.view().view_rect_local();
        render.fill("background", area, ' ')?;
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }

        render.text("greeting", area.line(0)?, "Hello, Canopy!")?;
        if area.h > 1 {
            render.text("count", area.line(1)?, &format!("count: {}", self.count))?;
        }
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        context.add_child(
            context.node_id(),
            StatusBar::new()
                .with_left(Text::new("hello"))
                .with_right(KeyHint::for_command(Root::call_toggle_help(), "help")),
        )?;
        context.set_focus(context.node_id()).map(|_| ())
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("hello")
    }
}

impl Register for Hello {
    fn register(setup: &mut Setup) -> Result<()> {
        setup.add_commands::<Self>()
    }
}

/// Create the full Canopy application.
///
/// A `home` holding `init.luau` is mounted as the trusted user configuration;
/// otherwise the application runs [`DEFAULT_CONFIG`]. Headless and API launch
/// modes pass `None` so they never read user state.
pub fn create_app(home: Option<&ConfigHome>) -> Result<Canopy> {
    CanopyBuilder::new()
        .configure(|setup| {
            Root::register(setup)?;
            Hello::register(setup)?;
            setup.widget_styles(install_styles);
            Ok(())
        })
        .assemble(|canopy| {
            Root::new().install(canopy, Hello::new())?;
            Ok(())
        })
        .user_config(home, DEFAULT_CONFIG)
        .build()
}

/// Install the per-element styles from the active palette.
fn install_styles(palette: &Palette, rules: StyleRules<'_>) {
    rules.fg("hello/greeting", palette.accent).apply();
}

#[cfg(test)]
mod tests {
    use canopy::{geom::Size, testing::harness::Harness};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn app_renders_the_greeting_and_mounts_focus() -> anyhow::Result<()> {
        let canopy = create_app(None)?;
        let mut harness = Harness::from_canopy(canopy, Size::new(40, 6))?;
        harness.render()?;
        assert!(harness.tbuf().contains_text("Hello, Canopy!"));
        assert!(harness.tbuf().contains_text("count: 0"));
        assert!(harness.tbuf().contains_text("ctrl+g: help"));
        assert!(
            harness
                .canopy
                .with_root_view(|view| view.focused_node())
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn a_written_home_loads_the_default_bindings() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let home = ConfigHome::new(directory.path());
        assert!(home.write_defaults(DEFAULT_CONFIG)?);
        let canopy = create_app(Some(&home))?;
        let mut harness = Harness::from_canopy(canopy, Size::new(40, 6))?;
        harness.render()?;
        harness.script(
            r#"
            canopy.send_key("+")
            canopy.prepare()
            canopy.assert(
                canopy.screen_text():find("count: 1") ~= nil,
                "the user + binding should run the command"
            )
            canopy.send_key("-")
            canopy.prepare()
            canopy.assert(
                canopy.screen_text():find("count: 0") ~= nil,
                "the user - binding should run the command"
            )
            "#,
        )?;
        Ok(())
    }
}
