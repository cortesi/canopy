#![deny(unsafe_code)]
#![warn(missing_docs)]
//! Example widgets used by canopy demos.

use canopy::{Canopy, Loader, Widget, error::Result, terminal::runloop};
use canopy_widgets::Root;

/// Shared global contextual-help trigger for Root-based demos.
const HELP_BINDING: &str = r#"
function setup()
    canopy.bind("?", {
        description = "Show key bindings",
        path = "/root/**/",
        tier = "global",
    }, function()
        root.toggle_help()
    end)
end
"#;

/// Char gym example nodes.
pub mod chargym;
/// Editor gym example nodes.
pub mod editorgym;
/// Focus gym example nodes.
pub mod focusgym;
/// Font gym example nodes.
pub mod fontgym;
/// Frame gym example nodes.
pub mod framegym;
/// Image viewer example nodes.
pub mod imgview;
/// Intervals example nodes.
pub mod intervals;
/// List gym example nodes.
pub mod listgym;
/// Pager example nodes.
pub mod pager;
/// Stylegym example nodes.
pub mod stylegym;
/// Terminal gym example nodes.
pub mod termgym;
/// Text gym example nodes.
pub mod textgym;
/// Widget demo nodes.
pub mod widget;
/// Widget editor example nodes.
pub mod widget_editor;

/// Finalize and print the Luau API definitions for a demo app.
pub fn print_luau_api(cnpy: &mut Canopy) -> Result<()> {
    cnpy.finalize_api()?;
    print!("{}", cnpy.script_api()?);
    Ok(())
}

/// Install the global contextual-help binding for one demo launcher.
pub fn install_help_binding(cnpy: &mut Canopy) -> Result<()> {
    cnpy.register_startup_script("examples-help", HELP_BINDING)
}

/// Install one demo app under a root and run the terminal loop.
pub fn run_demo<T: Widget + Loader + 'static>(
    mut cnpy: Canopy,
    app: T,
    inspector: bool,
) -> Result<i32> {
    Root::install_app_with_inspector(&mut cnpy, app, inspector)?;
    cnpy.run_startup_scripts()?;
    runloop(cnpy)
}

#[cfg(test)]
mod tests;
