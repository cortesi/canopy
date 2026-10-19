#![deny(unsafe_code)]
//! One launcher for every Canopy gym, driveable through MCP like the other
//! examples.

use std::{
    error::Error as StdError, fs, path::PathBuf, process::ExitCode, result::Result as StdResult,
};

use canopy::{
    Canopy, CanopyBuilder, Register, Widget,
    error::{Error, Result},
    input::key::Key,
    layout::{LayoutOverride, Sizing},
    style::{AttrSet, PartialStyle, StyleRules, themes::Palette},
    terminal::{InterruptPolicy, RunOptions},
};
use canopy_mcp::{AppFactory, AppMetadata, Error as McpError, LaunchMode, ResetPolicy, launch};
use canopy_widgets::{ImageView, Root};
use clap::{Parser, Subcommand};
use shell::DemoShell;

/// Label every demo footer gives the help key.
pub(crate) const HELP_LABEL: &str = "help";

/// Big text gym example nodes.
mod big;
/// Char gym example nodes.
mod char;
/// Chart gym example nodes.
mod chart;
/// Cursor gym example nodes.
mod cursor;
/// Editor gym example nodes.
mod editor;
/// Focus gym example nodes.
mod focus;
/// Font gym example nodes.
mod font;
/// Frame gym example nodes.
mod frame;
/// Image viewer example nodes.
mod imgview;
/// Intervals example nodes.
mod intervals;
/// List gym example nodes.
mod list;
/// Interactive Markdown and controlled streaming fixtures.
mod markdown;
/// Motion gym example nodes.
mod motion;
/// Pager example nodes.
mod pager;
/// Shared viewport controls and launcher footer.
mod shell;
/// Style gym example nodes.
mod style;
/// Keyed table navigation and rectangle selection.
mod table;
/// Terminal gym example nodes.
mod term;
/// Text gym example nodes.
mod text;
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
    CanopyBuilder::new()
        .configure(Root::register)
        .configure(DemoShell::register)
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
    /// Try custom text in every face, size, and setting of big text.
    Big,
    /// Browse the character and glyph rendering gym.
    Char,
    /// Watch the chart primitives update live in every theme.
    Chart,
    /// Compare every cursor role, motion, and shape.
    Cursor,
    /// Drive the experimental editor widget.
    Editor,
    /// Explore focus traversal across a node grid.
    Focus,
    /// Render large text with the ASCII font engine.
    Font,
    /// Explore frames, scrolling, and canvases.
    Frame,
    /// View an image in the terminal.
    Imgview {
        /// Image to display.
        file: PathBuf,
    },
    /// Watch periodic poll callbacks drive a list.
    Intervals,
    /// Explore the list widget.
    List,
    /// Read Markdown, inspect cards, and publish controlled stream chunks.
    Markdown,
    /// Watch colors change over time.
    Motion,
    /// Page through a text file.
    Pager {
        /// File to page.
        file: PathBuf,
    },
    /// Explore themes and render effects.
    Style,
    /// Inspect a keyed table with two-axis navigation and exact copying.
    Table,
    /// Run a shell inside the terminal widget.
    Term,
    /// Explore text wrapping and tab expansion.
    Text,
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
            Self::Big => big::binding_setup(builder.configure(big::BigGym::register)),
            Self::Char => char::binding_setup(builder),
            Self::Chart => chart::binding_setup(builder.configure(chart::ChartGym::register)),
            Self::Cursor => cursor::binding_setup(builder),
            Self::Editor => editor::binding_setup(builder.configure(editor::EditorGym::register)),
            Self::Focus => focus::binding_setup(builder.configure(focus::FocusGym::register)),
            Self::Font => font::binding_setup(builder.configure(font::FontGym::register)),
            Self::Frame => frame::binding_setup(builder.configure(frame::FrameGym::register)),
            Self::Imgview { .. } => imgview::binding_setup(builder),
            Self::Intervals => {
                intervals::binding_setup(builder.configure(intervals::Intervals::register))
            }
            Self::List => list::binding_setup(builder.configure(list::ListGym::register)),
            Self::Markdown => {
                markdown::binding_setup(builder.configure(markdown::MarkdownGym::register))
            }
            Self::Motion => motion::binding_setup(builder),
            Self::Pager { .. } => pager::binding_setup(builder),
            Self::Style => style::binding_setup(builder.configure(style::Stylegym::register)),
            Self::Table => table::binding_setup(builder.configure(table::TableGym::register)),
            Self::Term => term::binding_setup(builder.configure(term::TermGym::register)),
            Self::Text => text::binding_setup(builder),
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
            Self::Big => install(builder, big::BigGym::new(), inspector),
            Self::Char => install(builder, char::CharGym::new(), inspector),
            Self::Chart => install(builder, chart::ChartGym::new(), inspector),
            Self::Cursor => install(builder, cursor::CursorGym::new(), inspector),
            Self::Editor => install(builder, editor::EditorGym::new(), inspector),
            Self::Focus => install(builder, focus::FocusGym::new(), inspector),
            Self::Font => install(builder, font::FontGym::new(), inspector),
            Self::Frame => install(builder, frame::FrameGym::new(), inspector),
            Self::Imgview { file } => install(
                builder,
                imgview::ImgGym::new(ImageView::from_path(file)?),
                inspector,
            ),
            Self::Intervals => install(builder, intervals::Intervals::new(), inspector),
            Self::List => install(builder, list::ListGym::new(), inspector),
            Self::Markdown => install(builder, markdown::MarkdownGym::new(), inspector),
            Self::Motion => install(builder, motion::MotionGym::new(), inspector),
            Self::Pager { file } => install(builder, pager::Pager::new(&read(file)?), inspector),
            Self::Style => install(builder, style::Stylegym::new(), inspector),
            Self::Table => install(builder, table::TableGym::new(), inspector),
            Self::Term => install(builder, term::TermGym::new(), inspector),
            Self::Text => install(builder, text::TextGym::new(), inspector),
        };
        builder.build()
    }

    /// Return how the terminal loop treats interrupts for this demo.
    ///
    /// The terminal gym routes Ctrl+C to its shell, with an emergency exit.
    fn run_options(&self) -> Result<RunOptions> {
        Ok(match self {
            Self::Term => RunOptions {
                interrupt_policy: InterruptPolicy::RouteToApplication,
                emergency_exit: Some(Key::parse_spec("Ctrl+Alt+q")?),
                ..RunOptions::default()
            },
            _ => RunOptions::default(),
        })
    }
}
