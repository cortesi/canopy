#![deny(unsafe_code)]
//! One launcher for every Canopy gym, driveable through MCP like the other
//! examples.

use std::{
    error::Error as StdError, fs, path::PathBuf, process::ExitCode, result::Result as StdResult,
};

use canopy::{
    Canopy, CanopyBuilder, Context, ContextExt, Register, Widget,
    error::{Error, Result},
    input::key::Key,
    layout::{Direction, Layout, LayoutOverride, Sizing},
    style::{AttrSet, PartialStyle, StyleRules, themes::Palette},
    terminal::{InterruptPolicy, RunOptions},
};
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, LaunchMode, ResetPolicy, launch};
use canopy_widgets::{ImageView, KeyHint, Root, StatusBar, Text};
use clap::{Parser, Subcommand};

/// Label every demo footer gives the help key.
pub(crate) const HELP_LABEL: &str = "help";

/// Char gym example nodes.
mod chargym;
mod cursorgym;
/// Editor gym example nodes.
mod editorgym;
/// Focus gym example nodes.
mod focusgym;
/// Font gym example nodes.
mod fontgym;
/// Frame gym example nodes.
mod framegym;
/// Image viewer example nodes.
mod imgview;
/// Intervals example nodes.
mod intervals;
/// List gym example nodes.
mod listgym;
mod motiongym;
/// Pager example nodes.
mod pager;
/// Stylegym example nodes.
mod stylegym;
/// Terminal gym example nodes.
mod termgym;
/// Text gym example nodes.
mod textgym;
/// Widget editor example nodes.
mod widget_editor;

/// Add the standard normal and selected entry rules under a path prefix.
pub(crate) fn selectable_entry_styles<'a>(
    palette: &Palette,
    rules: StyleRules<'a>,
    prefix: &str,
) -> StyleRules<'a> {
    let selected_attrs = AttrSet {
        bold: true,
        ..AttrSet::default()
    };
    let normal = PartialStyle::new().fg(palette.fg).bg(palette.bg);
    let selected = PartialStyle::new()
        .fg(palette.bg)
        .bg(palette.accent)
        .attrs(selected_attrs);
    rules
        .prefix(prefix)
        .style_all(&["border", "fill", "text"], normal)
        .style_all(
            &["selected/border", "selected/fill", "selected/text"],
            selected,
        )
}

/// Navigation keys for a full-window scrollable demo.
///
/// The keys offer the navigation intents, which scroll whichever view on the
/// focus route can move. The wheel scrolls by default.
pub(crate) const TEXT_SCROLL_BINDINGS: &str = r#"
root.default_bindings()
canopy.keymap({
    { key = "g", description = "Top", action = "canopy.nav.first" },
    { key = { "j", "Down" }, description = "Scroll down", action = "canopy.nav.down" },
    { key = { "k", "Up" }, description = "Scroll up", action = "canopy.nav.up" },
    { key = { "h", "Left" }, description = "Scroll left", action = "canopy.nav.left" },
    { key = { "l", "Right" }, description = "Scroll right", action = "canopy.nav.right" },
    { key = { "PageDown", "Space" }, description = "Page down", action = "canopy.nav.page_down" },
    { key = "PageUp", description = "Page up", action = "canopy.nav.page_up" },
})
"#;

/// Start demo registration with Root, whose default bindings every gym's
/// script installs first.
#[must_use]
fn demo_canopy() -> CanopyBuilder {
    CanopyBuilder::new().configure(Root::register)
}

/// Install `app` under a root with the demo shell, as the builder's assembly.
fn install<T: Widget + 'static>(builder: CanopyBuilder, app: T, inspector: bool) -> CanopyBuilder {
    builder.assemble(move |cnpy| {
        Root::new()
            .with_inspector(inspector)
            .install(cnpy, DemoShell::new(app))?;
        Ok(())
    })
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
                .with_left(Text::new(self.status.clone()))
                .with_right(KeyHint::for_command(Root::call_toggle_help(), HELP_LABEL)),
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

/// Shared CLI flags for every demo.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long, global = true)]
    api: bool,

    /// Enable the inspector overlay.
    #[clap(short, long, global = true)]
    inspector: bool,

    /// Serve headless MCP automation over stdio.
    #[clap(long, global = true)]
    headless: bool,

    /// Serve live MCP automation over this Unix-domain socket.
    #[clap(long, global = true)]
    mcp: Option<PathBuf>,

    /// The demo to run.
    #[command(subcommand)]
    demo: Demo,
}

/// Every demo this launcher can run.
#[derive(Subcommand, Debug, Clone)]
enum Demo {
    /// Edit a file with vi keys and syntax highlighting.
    Cedit {
        /// File to edit.
        file: PathBuf,
    },
    /// Browse the character and glyph rendering gym.
    Chargym,
    /// Compare every cursor role, motion, and shape.
    Cursorgym,
    /// Drive the experimental editor widget.
    Editorgym,
    /// Explore focus traversal across a node grid.
    Focusgym,
    /// Render large text with the ASCII font engine.
    Fontgym,
    /// Explore frames, scrolling, and canvases.
    Framegym,
    /// View an image in the terminal.
    Imgview {
        /// Image to display.
        file: PathBuf,
    },
    /// Watch periodic poll callbacks drive a list.
    Intervals,
    /// Explore the list widget.
    Listgym,
    /// Watch colors change over time.
    Motiongym,
    /// Page through a text file.
    Pager {
        /// File to page.
        file: PathBuf,
    },
    /// Explore themes and render effects.
    Stylegym,
    /// Run a shell inside the terminal widget.
    Termgym,
    /// Explore text wrapping and tab expansion.
    Textgym,
}

/// Run one demo, interactively or under MCP automation.
fn main() -> StdResult<ExitCode, Box<dyn StdError>> {
    let args = Args::parse();
    if args.api {
        print!("{}", args.demo.build(args.inspector)?.script_api()?);
        return Ok(ExitCode::SUCCESS);
    }
    let mode = if args.headless {
        LaunchMode::HeadlessMcp
    } else {
        LaunchMode::Run {
            mcp_socket: args.mcp,
            options: args.demo.run_options()?,
        }
    };
    let demo = args.demo;
    let inspector = args.inspector;
    let factory = AppFactory::new(
        AppMetadata {
            app: "gyms".into(),
            reset: ResetPolicy::Isolated,
        },
        move || demo.build(inspector).map_err(McpError::app),
    );
    Ok(launch(factory, mode)?)
}

impl Demo {
    /// Queue the demo's API registration and binding setup.
    fn configure(&self, builder: CanopyBuilder) -> CanopyBuilder {
        match self {
            Self::Cedit { .. } => widget_editor::binding_setup(
                builder.configure(widget_editor::WidgetEditor::register),
            ),
            Self::Chargym => chargym::binding_setup(builder.configure(chargym::CharGym::register)),
            Self::Cursorgym => cursorgym::binding_setup(builder),
            Self::Editorgym => {
                editorgym::binding_setup(builder.configure(editorgym::EditorGym::register))
            }
            Self::Focusgym => {
                focusgym::binding_setup(builder.configure(focusgym::FocusGym::register))
            }
            Self::Fontgym => fontgym::binding_setup(builder),
            Self::Framegym => {
                framegym::binding_setup(builder.configure(framegym::FrameGym::register))
            }
            Self::Imgview { .. } => imgview::binding_setup(builder.configure(ImageView::register)),
            Self::Intervals => {
                intervals::binding_setup(builder.configure(intervals::Intervals::register))
            }
            Self::Listgym => listgym::binding_setup(builder.configure(listgym::ListGym::register)),
            Self::Motiongym => motiongym::binding_setup(builder),
            Self::Pager { .. } => pager::binding_setup(builder.configure(pager::Pager::register)),
            Self::Stylegym => {
                stylegym::binding_setup(builder.configure(stylegym::Stylegym::register))
            }
            Self::Termgym => termgym::binding_setup(builder.configure(termgym::TermGym::register)),
            Self::Textgym => textgym::binding_setup(builder),
        }
    }

    /// Build the demo application: its registration, bindings, and root.
    fn build(&self, inspector: bool) -> Result<Canopy> {
        let builder = self.configure(demo_canopy());
        let read =
            |file: &PathBuf| fs::read_to_string(file).map_err(|error| Error::App(Box::new(error)));
        let builder = match self {
            Self::Cedit { file } => install(
                builder,
                widget_editor::WidgetEditor::new(
                    read(file)?,
                    widget_editor::file_extension(file),
                    widget_editor::file_title(file),
                ),
                inspector,
            ),
            Self::Chargym => install(builder, chargym::CharGym::new(), inspector),
            Self::Cursorgym => install(builder, cursorgym::CursorGym::new(), inspector),
            Self::Editorgym => install(builder, editorgym::EditorGym::new(), inspector),
            Self::Focusgym => install(builder, focusgym::FocusGym::new(), inspector),
            Self::Fontgym => install(builder, fontgym::FontGym::new(), inspector),
            Self::Framegym => install(builder, framegym::FrameGym::new(), inspector),
            Self::Imgview { file } => install(builder, ImageView::from_path(file)?, inspector),
            Self::Intervals => install(builder, intervals::Intervals::new(), inspector),
            Self::Listgym => install(builder, listgym::ListGym::new(), inspector),
            Self::Motiongym => install(builder, motiongym::MotionGym::new(), inspector),
            Self::Pager { file } => install(builder, pager::Pager::new(&read(file)?), inspector),
            Self::Stylegym => install(builder, stylegym::Stylegym::new(), inspector),
            Self::Termgym => install(builder, termgym::TermGym::new(), inspector),
            Self::Textgym => install(builder, textgym::TextGym::new(), inspector),
        };
        builder.build()
    }

    /// Return how the terminal loop treats interrupts for this demo.
    ///
    /// The terminal gym routes Ctrl+C to its shell, with an emergency exit.
    fn run_options(&self) -> Result<RunOptions> {
        Ok(match self {
            Self::Termgym => RunOptions {
                interrupt_policy: InterruptPolicy::RouteToApplication,
                emergency_exit: Some(Key::parse_spec("Ctrl+Alt+q")?),
                ..RunOptions::default()
            },
            _ => RunOptions::default(),
        })
    }
}
