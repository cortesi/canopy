//! Launch the widget demo application.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    event::key,
    prelude::*,
};
use canopy_examples::widget::{DemoHost, DemoSize, FontDemo, FontSource, ListDemo};
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
    /// Print available commands and exit.
    #[clap(short, long)]
    commands: bool,

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

/// Run the widget demo.
fn main() -> Result<()> {
    let args = Args::parse();
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;

    if args.commands {
        cnpy.print_command_table(&mut io::stdout(), false)?;
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
            ImageView::load(&mut cnpy)?;
            setup_image_bindings(&mut cnpy);
            let view = ImageView::from_path(&image_args.path)?;
            DemoHost::new(view, size, true)
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
    };
    let app_id = cnpy.core.create_detached(demo);

    Root::install_with_inspector(&mut cnpy.core, app_id, args.inspector)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
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

/// Register keybindings for image zooming and panning.
fn setup_image_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .key('q', "root::quit()")
        .with_path("image_view/")
        .key('i', "image_view::zoom_in()")
        .key('o', "image_view::zoom_out()")
        .key('h', "image_view::pan_left()")
        .key('j', "image_view::pan_down()")
        .key('k', "image_view::pan_up()")
        .key('l', "image_view::pan_right()")
        .key(key::KeyCode::Left, "image_view::pan_left()")
        .key(key::KeyCode::Right, "image_view::pan_right()")
        .key(key::KeyCode::Up, "image_view::pan_up()")
        .key(key::KeyCode::Down, "image_view::pan_down()");
}
