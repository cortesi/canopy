#![deny(unsafe_code)]
//! Minimal Canopy application, and the reference for building one outside the
//! Canopy workspace.
//!
//! The crate shows the whole external contract in one place: a widget with a
//! command, a [`CanopyBuilder`] pipeline, an optional trusted user
//! configuration directory, and a factory that every launch mode shares.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use canopy::{
    Canopy, CanopyBuilder, Context, Loader, NodeName, Render, ScriptTrust, ViewContext, Widget,
    derive_commands,
    error::Result,
    layout::Layout,
    style::{StyleBuilder, solarized},
};
use canopy_widgets::Root;

/// Default keymap copied to a user's configuration directory on first use.
pub const DEFAULT_BINDINGS: &str = include_str!("default_bindings.luau");

/// Startup module that loads the editable user keymap.
pub const DEFAULT_INIT: &str = r#"local bindings = require("./bindings")

function setup()
    bindings.setup()
end
"#;

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
        Layout::fill()
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
        if area.h > 2 {
            render.text(
                "status",
                area.line(area.h - 1)?,
                " +/- count  ? help  q quit ",
            )?;
        }
        Ok(())
    }

    fn on_mount(&mut self, context: &mut dyn Context) -> Result<()> {
        context.set_focus(context.node_id()).map(|_| ())
    }

    fn accept_focus(&self, _context: &dyn ViewContext) -> bool {
        true
    }

    fn name(&self) -> NodeName {
        NodeName::convert("hello")
    }
}

impl Loader for Hello {
    fn load(canopy: &mut Canopy) -> Result<()> {
        canopy.add_commands::<Self>()
    }
}

/// Create the full Canopy application.
///
/// Pass `Some(root)` to mount a trusted user configuration directory, and
/// `None` to fall back to the compiled-in defaults. Headless and API launch
/// modes must always pass `None` so they never read or create user state.
pub fn create_app(user_script_root: Option<PathBuf>) -> Result<Canopy> {
    let mut builder = CanopyBuilder::new()
        .configure(|canopy| {
            Root::load(canopy)?;
            Hello::load(canopy)?;
            install_styles(canopy);
            Ok(())
        })
        .assemble(|canopy| {
            Root::new().install(canopy, Hello::new())?;
            Ok(())
        });

    if let Some(root) = user_script_root {
        builder = builder.user_script_root(root, ScriptTrust::TrustedLocal);
    } else {
        builder = builder.bindings("hello-defaults", DEFAULT_BINDINGS);
    }
    builder.build()
}

/// Create the default user config without replacing existing files.
///
/// A first run writes `init.luau` and `bindings.luau`. Every later run leaves
/// whatever the user has since edited in place.
pub fn ensure_user_config(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    create_new_file(&root.join("init.luau"), DEFAULT_INIT)?;
    let module = format!(
        "local bindings = {{}}\n\nfunction bindings.setup()\n{}\nend\n\nreturn bindings\n",
        indent(DEFAULT_BINDINGS, "    ")
    );
    create_new_file(&root.join("bindings.luau"), &module)
}

/// Create a file atomically with respect to concurrent first runs.
///
/// `create_new` is the point of this helper. `fs::write` would silently
/// replace a configuration file the user had already edited.
fn create_new_file(path: &Path, contents: &str) -> io::Result<()> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(contents.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// Indent non-empty source lines for a generated Luau function body.
fn indent(source: &str, prefix: &str) -> String {
    source
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Install the application palette and per-element styles.
fn install_styles(canopy: &mut Canopy) {
    *canopy.style_mut() = solarized::solarized_dark();
    canopy
        .style_mut()
        .rules()
        .fg("hello/greeting", solarized::BLUE)
        .style(
            "hello/status",
            StyleBuilder::new()
                .fg(solarized::BASE1)
                .bg(solarized::BASE02),
        )
        .apply();
}

#[cfg(test)]
mod tests {
    use std::fs;

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
        assert!(
            harness
                .canopy
                .with_root_view(|view| view.focused_node())
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn first_run_config_preserves_existing_bindings() -> anyhow::Result<()> {
        let directory = tempdir()?;
        ensure_user_config(directory.path())?;
        let bindings = directory.path().join("bindings.luau");
        fs::write(&bindings, "return { setup = function() end }")?;
        ensure_user_config(directory.path())?;
        assert_eq!(
            fs::read_to_string(bindings)?,
            "return { setup = function() end }"
        );
        assert!(fs::read_to_string(directory.path().join("init.luau"))?.contains("setup"));
        Ok(())
    }

    #[test]
    fn persistent_user_root_loads_default_bindings() -> anyhow::Result<()> {
        let directory = tempdir()?;
        ensure_user_config(directory.path())?;
        let canopy = create_app(Some(directory.path().to_path_buf()))?;
        let mut harness = Harness::from_canopy(canopy, Size::new(40, 6))?;
        harness.render()?;
        harness.script(
            r#"
            canopy.send_key("+")
            canopy.flush()
            canopy.assert(
                canopy.screen_text():find("count: 1") ~= nil,
                "the user + binding should run the command"
            )
            canopy.send_key("-")
            canopy.flush()
            canopy.assert(
                canopy.screen_text():find("count: 0") ~= nil,
                "the user - binding should run the command"
            )
            "#,
        )?;
        Ok(())
    }
}
