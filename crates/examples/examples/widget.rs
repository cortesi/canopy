//! Launch the widget demo application.

use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    print_luau_api,
    widget::{DemoHost, DemoSize, FontDemo, FontSource, ListDemo, TermDemo},
    widget_editor::{WidgetEditor, setup_bindings},
};
use canopy_widgets::{FontEffects, ImageView, Root};
use clap::{Parser, Subcommand};
use unicode_width::UnicodeWidthStr;

/// Default text for the font demo.
const DEFAULT_TEXT: &str = "Canopy";
/// Default font directory for the font demo.
const DEFAULT_FONT_DIR: &str = "assets/fonts";
/// Default interval for switching fonts, in milliseconds.
const DEFAULT_FONT_INTERVAL_MS: u64 = 1000;
/// Default image path for the image demo.
const DEFAULT_IMAGE_PATH: &str = "assets/tiger.jpg";
/// Default interval for list selection changes, in milliseconds.
const DEFAULT_LIST_INTERVAL_MS: u64 = 500;
/// Default Rust file to open in the widget editor.
const DEFAULT_SOURCE_PATH: &str = "crates/canopy-widgets/src/button.rs";
/// Items used in the list demo.
const LIST_ITEMS: [&str; 5] = [
    "Item One",
    "Item Two",
    "Item Three",
    "Item Four",
    "Item Five",
];

/// CLI flags for the widget demo.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition for the selected demo and exit.
    #[clap(long)]
    api: bool,

    /// Enable the inspector overlay.
    #[clap(short, long)]
    inspector: bool,

    /// Override demo width (columns).
    #[clap(long, value_name = "COLUMNS")]
    width: Option<u32>,

    /// Override demo height (rows).
    #[clap(long, value_name = "ROWS")]
    height: Option<u32>,

    /// Wrap the demo in a frame.
    #[clap(long)]
    frame: bool,

    /// Subcommand selecting the widget demo to run.
    #[command(subcommand)]
    command: Command,
}

/// Widget demo subcommands.
#[derive(Subcommand, Debug)]
enum Command {
    /// Render text using the font widget.
    Font(FontArgs),
    /// Render an image using the image viewer.
    Image(ImageArgs),
    /// Render a simple list demo.
    List(ListArgs),
    /// Render a terminal demo with tabs.
    Term,
    /// Open a source file in the widget editor.
    Editor(EditorArgs),
}

/// Arguments for the list widget demo.
#[derive(Parser, Debug)]
struct ListArgs {
    /// Selection advance interval in milliseconds.
    #[arg(long, value_name = "MILLISECONDS", default_value_t = DEFAULT_LIST_INTERVAL_MS)]
    interval_ms: u64,
}

/// Arguments for the font widget demo.
#[derive(Parser, Debug)]
struct FontArgs {
    /// Text to render.
    #[arg(value_name = "TEXT", default_value = DEFAULT_TEXT)]
    text: String,

    /// Interval between font switches in milliseconds.
    #[arg(long, value_name = "MILLISECONDS", default_value_t = DEFAULT_FONT_INTERVAL_MS)]
    interval_ms: u64,

    /// Exit after each font has been displayed once.
    #[arg(long)]
    exit_after_cycle: bool,

    /// Render bold text.
    #[arg(long)]
    bold: bool,

    /// Render italic text.
    #[arg(long)]
    italic: bool,

    /// Render underlined text.
    #[arg(long)]
    underline: bool,

    /// Render dimmed text.
    #[arg(long)]
    dim: bool,

    /// Render overlined text.
    #[arg(long)]
    overline: bool,

    /// Render struck-through text.
    #[arg(long)]
    strike: bool,
}

/// Arguments for the image widget demo.
#[derive(Parser, Debug)]
struct ImageArgs {
    /// Path to an image file.
    #[arg(long, value_name = "PATH", default_value = DEFAULT_IMAGE_PATH)]
    path: PathBuf,
}

/// Arguments for the widget editor demo.
#[derive(Parser, Debug)]
struct EditorArgs {
    /// Path to the file to open.
    #[arg(long, value_name = "PATH", default_value = DEFAULT_SOURCE_PATH)]
    path: PathBuf,
}

/// Run the widget demo.
fn main() -> Result<()> {
    let args = Args::parse();
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;

    load_widget_api(&mut cnpy, &args.command)?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    let size = DemoSize::new(args.width, args.height);
    let demo = match args.command {
        Command::Font(font_args) => {
            let fonts = load_font_sources(Path::new(DEFAULT_FONT_DIR))?;
            let interval = Duration::from_millis(font_args.interval_ms.max(1));
            let effects = FontEffects {
                bold: font_args.bold,
                italic: font_args.italic,
                underline: font_args.underline,
                dim: font_args.dim,
                overline: font_args.overline,
                strike: font_args.strike,
            };
            DemoHost::new(
                FontDemo::new(
                    font_args.text,
                    fonts,
                    interval,
                    font_args.exit_after_cycle,
                    effects,
                ),
                size,
                args.frame,
            )
        }
        Command::Image(image_args) => {
            setup_image_bindings(&mut cnpy);
            let view = ImageView::from_path(&image_args.path)?;
            DemoHost::new(view, size, true)
                .with_inner_padding(0)
                .with_outer_padding(1)
        }
        Command::List(list_args) => {
            let interval = Duration::from_millis(list_args.interval_ms.max(1));
            let size = if args.frame {
                let (list_width, list_height) = list_demo_size();
                DemoSize::new(
                    args.width.or(Some(list_width)),
                    args.height.or(Some(list_height)),
                )
            } else {
                size
            };
            DemoHost::new(ListDemo::new(interval), size, args.frame)
        }
        Command::Term => {
            setup_term_bindings(&mut cnpy);
            DemoHost::new(TermDemo::new(), size, args.frame)
        }
        Command::Editor(editor_args) => {
            let contents = fs::read_to_string(&editor_args.path)
                .map_err(|err| error::Error::Internal(err.to_string()))?;
            let extension = file_extension(&editor_args.path);
            let title = file_title(&editor_args.path);

            setup_bindings(&mut cnpy);

            DemoHost::new(
                WidgetEditor::new(contents, extension, title),
                size,
                args.frame,
            )
            .with_inner_padding(0)
        }
    };
    let app_id = cnpy.core.create_detached(demo);

    Root::install_with_inspector(&mut cnpy.core, app_id, args.inspector)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}

/// Load the command surface needed by the selected widget demo.
fn load_widget_api(cnpy: &mut Canopy, command: &Command) -> Result<()> {
    match command {
        Command::Font(_) | Command::List(_) => Ok(()),
        Command::Image(_) => ImageView::load(cnpy),
        Command::Term => cnpy.add_commands::<TermDemo>(),
        Command::Editor(_) => WidgetEditor::load(cnpy),
    }
}

/// Compute the list demo width and height.
fn list_demo_size() -> (u32, u32) {
    let mut max_width = 1u32;
    for item in LIST_ITEMS {
        let width = UnicodeWidthStr::width(item) as u32 + 1;
        max_width = max_width.max(width);
    }
    let height = LIST_ITEMS.len().max(1) as u32;
    (max_width, height)
}

/// Load fonts from a directory, returning stable ordering by filename.
fn load_font_sources(path: &Path) -> Result<Vec<FontSource>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .map_err(|err| {
            error::Error::Invalid(format!(
                "failed to read font directory {}: {err}",
                path.display()
            ))
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf"))
        })
        .collect();
    entries.sort();

    let mut sources = Vec::new();
    for path in entries {
        let bytes = fs::read(&path).map_err(|err| {
            error::Error::Invalid(format!("failed to read font {}: {err}", path.display()))
        })?;
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("font")
            .to_string();
        sources.push(FontSource::new(label, bytes));
    }

    if sources.is_empty() {
        return Err(error::Error::Invalid(format!(
            "no fonts found in {}",
            path.display()
        )));
    }
    Ok(sources)
}

/// Return a lowercase file extension hint for syntax selection.
fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "txt".to_string())
}

/// Return a short title for the editor frame.
fn file_title(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Register keybindings for image zooming and panning.
fn setup_image_bindings(cnpy: &mut Canopy) {
    cnpy.run_default_script(
        r#"
canopy.bind_with("q", { desc = "Quit" }, function()
    root.quit()
end)
canopy.bind_with("i", { path = "image_view/", desc = "Zoom in" }, function()
    image_view.zoom_in()
end)
canopy.bind_with("o", { path = "image_view/", desc = "Zoom out" }, function()
    image_view.zoom_out()
end)
canopy.bind_with("h", { path = "image_view/", desc = "Pan left" }, function()
    image_view.pan_left()
end)
canopy.bind_with("j", { path = "image_view/", desc = "Pan down" }, function()
    image_view.pan_down()
end)
canopy.bind_with("k", { path = "image_view/", desc = "Pan up" }, function()
    image_view.pan_up()
end)
canopy.bind_with("l", { path = "image_view/", desc = "Pan right" }, function()
    image_view.pan_right()
end)
canopy.bind_with("Left", { path = "image_view/", desc = "Pan left" }, function()
    image_view.pan_left()
end)
canopy.bind_with("Right", { path = "image_view/", desc = "Pan right" }, function()
    image_view.pan_right()
end)
canopy.bind_with("Up", { path = "image_view/", desc = "Pan up" }, function()
    image_view.pan_up()
end)
canopy.bind_with("Down", { path = "image_view/", desc = "Pan down" }, function()
    image_view.pan_down()
end)
"#,
    )
    .unwrap();
}

/// Register keybindings for the terminal demo.
fn setup_term_bindings(cnpy: &mut Canopy) {
    cnpy.run_default_script(
        r#"
canopy.bind_with("ctrl-Tab", { path = "term_demo/**/", desc = "Next tab" }, function()
    term_demo.next_tab()
end)
"#,
    )
    .unwrap();
}
