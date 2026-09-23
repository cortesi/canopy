#![deny(unsafe_code)]
//! Example widgets used by canopy demos.

use canopy::{
    CanopyBuilder, Context, ContextExt, FocusDirection, Register, Widget,
    error::{Error, Result},
    layout::{Direction, Layout, LayoutOverride, Sizing},
    style::{
        AttrSet, Color, GradientSpec, GradientStop, Paint, StyleBuilder, StyleRules,
        default as palette,
    },
    terminal::{RunOptions, runloop},
};
use canopy_widgets::{KeyHint, Root, StatusBar, Text};

/// Shared global contextual-help trigger for Root-based demos.
///
/// `Ctrl+g` is the standard Canopy help key. A control chord reaches the
/// binding even while a field has focus, where `?` is a character to insert.
const HELP_BINDING: &str = r#"
function setup()
    canopy.bind("Ctrl+g", {
        path = "/root/**/",
        phase = "before_widget",
        tier = "global",
        description = "Show key bindings",
    }, command.root.toggle_help())
end
"#;

/// Key every demo footer hints at.
pub(crate) const HELP_KEY: &str = "ctrl-g";
/// Label every demo footer gives that key.
pub(crate) const HELP_LABEL: &str = "help";

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
    let normal = StyleBuilder::new().fg(palette::TEXT).bg(palette::BG);
    let selected = StyleBuilder::new()
        .fg(palette::BG)
        .bg(palette::ACCENT)
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
canopy.keymap({
    path = "{path}",
    { key = "g", description = "Top", action = command.{receiver}.scroll_to(0, 0) },
    {
        key = { "j", "Down" },
        mouse = "ScrollDown",
        description = "Scroll down",
        action = command.{receiver}.scroll("down"),
    },
    {
        key = { "k", "Up" },
        mouse = "ScrollUp",
        description = "Scroll up",
        action = command.{receiver}.scroll("up"),
    },
    { key = { "h", "Left" }, description = "Scroll left", action = command.{receiver}.scroll("left") },
    { key = { "l", "Right" }, description = "Scroll right", action = command.{receiver}.scroll("right") },
    { key = { "PageDown", "Space" }, description = "Page down", action = command.{receiver}.page(1) },
    { key = "PageUp", description = "Page up", action = command.{receiver}.page(-1) },
})
canopy.bind("q", { path = "root", description = "Quit" }, command.root.quit())
"#;

/// Render the shared scroll bindings for one receiver and binding path.
pub(crate) fn text_scroll_bindings(receiver: &str, path: &str) -> String {
    TEXT_SCROLL_BINDINGS
        .replace("{receiver}", receiver)
        .replace("{path}", path)
}

/// Start demo registration with Root and its first-preparation help setup.
#[must_use]
pub fn demo_canopy() -> CanopyBuilder {
    CanopyBuilder::new().configure(|setup| {
        Root::register(setup)?;
        setup.register_startup_script("examples-help", HELP_BINDING)
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
            Root::new()
                .with_inspector(inspector)
                .install(cnpy, DemoShell::new(app))?;
            Ok(())
        })
        .build()?;
    runloop(canopy, options)
}

/// Wrap a demo app in the shared footer bar every launcher demo carries.
///
/// The footer names the app and pins the standard help hint to the right edge.
/// The app keeps the rest of the shell and its own layout; the shell only adds
/// the row beneath it.
struct DemoShell<T> {
    /// The demo application, mounted on first mount.
    app: Option<T>,
    /// Status text shown on the left of the footer.
    status: String,
}

impl<T: Widget + 'static> DemoShell<T> {
    /// Construct a shell around one demo app.
    fn new(app: T) -> Self {
        Self {
            status: app.name().to_string(),
            app: Some(app),
        }
    }
}

impl<T: Widget + 'static> Widget for DemoShell<T> {
    fn layout(&self) -> Layout {
        Layout::fill().direction(Direction::Column)
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let app = self
            .app
            .take()
            .ok_or_else(|| Error::Internal("demo app missing".into()))?;
        let app_node = c.add_child(c.node_id(), app)?;
        c.set_layout_override(
            app_node.into(),
            LayoutOverride {
                min_width: Some(None),
                max_width: Some(None),
                min_height: Some(None),
                max_height: Some(None),
                ..LayoutOverride::new().flex_horizontal(1).flex_vertical(1)
            },
        )?;
        c.add_child(
            c.node_id(),
            StatusBar::new()
                .with_left(Text::new(self.status.clone()).with_style("status_bar/text"))
                .with_right(KeyHint::new(HELP_KEY, HELP_LABEL)),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Override a column child to fill the width at a fixed outer height.
pub(crate) fn fixed_row(height: u32) -> LayoutOverride {
    LayoutOverride {
        height: Some(Sizing::Measure),
        ..LayoutOverride::new()
            .flex_horizontal(1)
            .fixed_height(height)
    }
}

/// Override a column child to fill the width and share the remaining height by
/// `weight`, without height bounds.
pub(crate) fn flex_row(weight: u32) -> LayoutOverride {
    LayoutOverride {
        min_height: Some(None),
        max_height: Some(None),
        ..LayoutOverride::new()
            .flex_horizontal(1)
            .flex_vertical(weight)
    }
}
