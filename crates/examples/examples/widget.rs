//! Launch the widget demo application.

use std::{
    fs,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use canopy::prelude::*;
use canopy_examples::{
    imgview, print_luau_api,
    widget::{DemoHost, DemoSize, FontDemo, FontSource, ListDemo, TermDemo},
    widget_editor::{self, WidgetEditor},
};
use canopy_widgets::{FontEffects, ImageView};
use clap::{Parser, Subcommand};

/// Default text for the font demo.
const DEFAULT_TEXT: &str = "Canopy";
/// Default font directory for the font demo.
const DEFAULT_FONT_DIR: &str = "crates/canopy-widgets/assets/fonts";
/// Default interval for switching fonts, in milliseconds.
const DEFAULT_FONT_INTERVAL_MS: u64 = 1000;
/// Default image path for the image demo.
const DEFAULT_IMAGE_PATH: &str = "assets/tiger.jpg";
/// Default interval for list selection changes, in milliseconds.
const DEFAULT_LIST_INTERVAL_MS: u64 = 500;
/// Default Rust file to open in the widget editor.
const DEFAULT_SOURCE_PATH: &str = "crates/canopy-widgets/src/button.rs";
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
    let mut cnpy = canopy_examples::demo_canopy()?;

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
            imgview::setup_bindings(&mut cnpy)?;
            let view = ImageView::from_path(&image_args.path)?;
            DemoHost::new(view, size, true)
                .with_inner_padding(0)
                .with_outer_padding(1)
        }
        Command::List(list_args) => {
            let interval = Duration::from_millis(list_args.interval_ms.max(1));
            let size = if args.frame {
                let (list_width, list_height) = ListDemo::natural_size();
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
            setup_term_bindings(&mut cnpy)?;
            DemoHost::new(TermDemo::new(), size, args.frame)
        }
        Command::Editor(editor_args) => {
            let contents = fs::read_to_string(&editor_args.path)
                .map_err(|err| error::Error::Internal(err.to_string()))?;
            let extension = widget_editor::file_extension(&editor_args.path);
            let title = widget_editor::file_title(&editor_args.path);

            widget_editor::setup_bindings(&mut cnpy)?;

            DemoHost::new(
                WidgetEditor::new(contents, extension, title),
                size,
                args.frame,
            )
            .with_inner_padding(0)
        }
    };
    let exit_code = canopy_examples::run_demo(cnpy, demo, args.inspector)?;
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

/// Load fonts from a directory, returning stable ordering by filename.
fn load_font_sources(path: &Path) -> Result<Vec<FontSource>> {
    let directory = fs::read_dir(path).map_err(|err| {
        error::Error::Invalid(format!(
            "failed to read font directory {}: {err}",
            path.display()
        ))
    })?;
    let mut entries = Vec::new();
    for entry in directory {
        let entry = entry.map_err(|err| {
            error::Error::Invalid(format!(
                "failed to read entry in font directory {}: {err}",
                path.display()
            ))
        })?;
        let entry_path = entry.path();
        if entry_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ttf"))
        {
            entries.push(entry_path);
        }
    }
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

/// Register keybindings for the terminal demo.
fn setup_term_bindings(cnpy: &mut Canopy) -> Result<()> {
    cnpy.eval_script(
        r#"
canopy.bind("ctrl-Tab", { path = "term_demo/**/", description = "Next tab" }, function()
    term_demo.next_tab()
end)
"#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use canopy::testing::dummyctx::DummyContext;

    use super::*;

    #[test]
    fn font_discovery_preserves_order_filters_extensions_and_rejects_empty_directories()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(format!(
            "canopy-font-discovery-{}-{}",
            process::id(),
            rand::random::<u64>(),
        ));
        fs::create_dir(&directory)?;
        let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
            fs::write(directory.join("z.ttf"), b"invalid font z")?;
            fs::write(directory.join("a.TTF"), b"invalid font a")?;
            fs::write(directory.join("ignored.txt"), b"not a font source")?;
            let sources = load_font_sources(&directory)?;
            assert_eq!(sources.len(), 2);
            for (source, name) in sources.into_iter().zip(["a.TTF", "z.ttf"]) {
                // FontDemo reports the selected source label when parsing
                // fails.
                let mut demo = FontDemo::new(
                    "test",
                    vec![source],
                    Duration::from_secs(1),
                    false,
                    FontEffects::default(),
                );
                let error = demo.on_mount(&mut DummyContext::default()).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(&format!("font parse failed for {name}:"))
                );
            }
            fs::remove_file(directory.join("a.TTF"))?;
            fs::remove_file(directory.join("z.ttf"))?;
            let error = load_font_sources(&directory).unwrap_err();
            assert!(error.to_string().contains("no fonts found in"));
            assert!(error.to_string().contains(directory.to_str().unwrap()));
            Ok(())
        })();
        fs::remove_dir_all(&directory)?;
        result
    }
}
