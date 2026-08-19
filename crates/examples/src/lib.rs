#![deny(unsafe_code)]
//! Example widgets used by canopy demos.

use canopy::{
    Canopy, Loader, Widget,
    error::Result,
    style::{Color, GradientSpec, GradientStop, Paint},
    terminal::runloop,
};
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

/// Build a four-stop gradient paint at a fixed angle, evenly weighted toward the tail.
pub(crate) fn banner_gradient(angle_deg: f32, colors: [Color; 4]) -> Paint {
    Paint::gradient(GradientSpec::with_stops(
        angle_deg,
        vec![
            GradientStop::new(0.0, colors[0]),
            GradientStop::new(0.35, colors[1]),
            GradientStop::new(0.7, colors[2]),
            GradientStop::new(1.0, colors[3]),
        ],
    ))
}

/// Binding template for a full-window `Text` demo, parameterized by the text node's path.
const TEXT_SCROLL_BINDINGS: &str = r#"
canopy.bind("g", { path = "{path}", description = "Top" }, function()
    text.scroll_to(0, 0)
end)
canopy.bind("j", { path = "{path}", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind("Down", { path = "{path}", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind_mouse("ScrollDown", { path = "{path}", description = "Scroll down" }, function()
    text.scroll("Down")
end)
canopy.bind("k", { path = "{path}", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind("Up", { path = "{path}", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind_mouse("ScrollUp", { path = "{path}", description = "Scroll up" }, function()
    text.scroll("Up")
end)
canopy.bind("h", { path = "{path}", description = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind("Left", { path = "{path}", description = "Scroll left" }, function()
    text.scroll("Left")
end)
canopy.bind("l", { path = "{path}", description = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind("Right", { path = "{path}", description = "Scroll right" }, function()
    text.scroll("Right")
end)
canopy.bind("PageDown", { path = "{path}", description = "Page down" }, function()
    text.page(1)
end)
canopy.bind("Space", { path = "{path}", description = "Page down" }, function()
    text.page(1)
end)
canopy.bind("PageUp", { path = "{path}", description = "Page up" }, function()
    text.page(-1)
end)
canopy.bind("q", { path = "root", description = "Quit" }, function()
    root.quit()
end)
"#;

/// Render the shared `Text` scroll bindings for one demo node path.
pub fn text_scroll_bindings(path: &str) -> String {
    TEXT_SCROLL_BINDINGS.replace("{path}", path)
}

/// Build a `Canopy` for a demo launcher, with `Root` loaded and the help binding installed.
pub fn demo_canopy() -> Result<Canopy> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    cnpy.register_startup_script("examples-help", HELP_BINDING)?;
    Ok(cnpy)
}

/// Install one demo app under a root and run the terminal loop.
pub fn run_demo<T: Widget + 'static>(mut cnpy: Canopy, app: T, inspector: bool) -> Result<i32> {
    Root::install_app_with_inspector(&mut cnpy, app, inspector)?;
    cnpy.run_startup_scripts()?;
    runloop(cnpy)
}

#[cfg(test)]
mod tests;
