#![deny(unsafe_code)]
//! Example widgets used by canopy demos.

use canopy::{
    CanopyBuilder, Context, FocusDirection, Loader, Widget,
    error::Result,
    style::{
        AttrSet, Color, GradientSpec, GradientStop, Paint, StyleBuilder, StyleRules, solarized,
    },
    terminal::{RunOptions, runloop_with_options},
};
use canopy_widgets::Root;

/// Shared global contextual-help trigger for Root-based demos.
const HELP_BINDING: &str = r#"
function setup()
    canopy.bind_command("?", { phase = "before_widget",
        description = "Show key bindings",
        path = "/root/**/",
        tier = "global",
    }, "root::toggle_help")
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

/// Build a four-stop gradient paint at a fixed angle, evenly weighted toward
/// the tail.
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

/// Scroll a context by one line in the given direction.
pub(crate) fn scroll_in(c: &mut dyn Context, dir: FocusDirection) {
    match dir {
        FocusDirection::Up | FocusDirection::Prev => c.scroll_up(),
        FocusDirection::Down | FocusDirection::Next => c.scroll_down(),
        FocusDirection::Left => c.scroll_left(),
        FocusDirection::Right => c.scroll_right(),
    };
}

/// Page a context by a signed delta; negative moves up, positive moves down.
pub(crate) fn page_by(c: &mut dyn Context, delta: i32) {
    if delta < 0 {
        c.page_up();
    } else if delta > 0 {
        c.page_down();
    }
}

/// Add the standard normal and selected entry rules under a path prefix.
pub(crate) fn selectable_entry_styles<'a>(rules: StyleRules<'a>, prefix: &str) -> StyleRules<'a> {
    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };
    let normal = StyleBuilder::new()
        .fg(solarized::BASE0)
        .bg(solarized::BASE03);
    let selected = StyleBuilder::new()
        .fg(solarized::BASE3)
        .bg(solarized::BLUE)
        .attrs(selected_attrs);
    rules
        .prefix(prefix)
        .style_all(&["border", "fill", "text"], normal)
        .style_all(
            &["selected/border", "selected/fill", "selected/text"],
            selected,
        )
}

/// Binding template for a full-window scrollable demo.
///
/// `{receiver}` is the Luau command owner. `{path}` is the binding path.
const TEXT_SCROLL_BINDINGS: &str = r#"
canopy.bind_command("g", { phase = "{phase}", path = "{path}", description = "Top" }, "{receiver}::scroll_to", 0, 0)
canopy.bind_command("j", { phase = "{phase}", path = "{path}", description = "Scroll down" }, "{receiver}::scroll", "Down")
canopy.bind_command("Down", { phase = "{phase}", path = "{path}", description = "Scroll down" }, "{receiver}::scroll", "Down")
canopy.bind_mouse("ScrollDown", { phase = "after_widget", path = "{path}", description = "Scroll down" }, function()
    {receiver}.scroll("Down")
end)
canopy.bind_command("k", { phase = "{phase}", path = "{path}", description = "Scroll up" }, "{receiver}::scroll", "Up")
canopy.bind_command("Up", { phase = "{phase}", path = "{path}", description = "Scroll up" }, "{receiver}::scroll", "Up")
canopy.bind_mouse("ScrollUp", { phase = "after_widget", path = "{path}", description = "Scroll up" }, function()
    {receiver}.scroll("Up")
end)
canopy.bind_command("h", { phase = "{phase}", path = "{path}", description = "Scroll left" }, "{receiver}::scroll", "Left")
canopy.bind_command("Left", { phase = "{phase}", path = "{path}", description = "Scroll left" }, "{receiver}::scroll", "Left")
canopy.bind_command("l", { phase = "{phase}", path = "{path}", description = "Scroll right" }, "{receiver}::scroll", "Right")
canopy.bind_command("Right", { phase = "{phase}", path = "{path}", description = "Scroll right" }, "{receiver}::scroll", "Right")
canopy.bind_command("PageDown", { phase = "{phase}", path = "{path}", description = "Page down" }, "{receiver}::page", 1)
canopy.bind_command("Space", { phase = "{phase}", path = "{path}", description = "Page down" }, "{receiver}::page", 1)
canopy.bind_command("PageUp", { phase = "{phase}", path = "{path}", description = "Page up" }, "{receiver}::page", -1)
canopy.bind_command("q", { phase = "after_widget", path = "root", description = "Quit" }, "root::quit")
"#;

/// Render the shared scroll bindings for one receiver and binding path.
pub(crate) fn text_scroll_bindings(receiver: &str, path: &str) -> String {
    TEXT_SCROLL_BINDINGS
        .replace("{receiver}", receiver)
        .replace("{path}", path)
        .replace(
            "{phase}",
            if path.ends_with('/') {
                "before_widget"
            } else {
                "after_widget"
            },
        )
}

/// Start demo registration with Root and its first-preparation help setup.
#[must_use]
pub fn demo_canopy() -> CanopyBuilder {
    CanopyBuilder::new().configure(|cnpy| {
        Root::load(cnpy)?;
        cnpy.register_startup_script("examples-help", HELP_BINDING)
    })
}

/// Install one demo app under a root and run the terminal loop.
pub fn run_demo<T: Widget + 'static>(
    builder: CanopyBuilder,
    app: T,
    inspector: bool,
) -> Result<i32> {
    run_demo_with_options(builder, app, inspector, RunOptions::default())
}

/// Install a demo and run it with explicit terminal interrupt behavior.
pub fn run_demo_with_options<T: Widget + 'static>(
    builder: CanopyBuilder,
    app: T,
    inspector: bool,
    options: RunOptions,
) -> Result<i32> {
    let canopy = builder
        .assemble(move |cnpy| {
            Root::new().with_inspector(inspector).install(cnpy, app)?;
            Ok(())
        })
        .build()?;
    runloop_with_options(canopy, options)
}

#[cfg(test)]
mod tests;
