//! Launch the widget editor example.

use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    error::Error,
    prelude::*,
};
use canopy_examples::widget_editor::{WidgetEditor, setup_bindings};
use canopy_widgets::Root;
use clap::Parser;

/// Default Rust file to open.
const DEFAULT_SOURCE_PATH: &str = "crates/canopy-widgets/src/button.rs";

/// CLI flags for the widget editor example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the Rust file to open.
    #[arg(long, value_name = "PATH", default_value = DEFAULT_SOURCE_PATH)]
    path: PathBuf,
}

/// Run the widget editor example.
fn main() -> Result<()> {
    let args = Args::parse();
    let contents =
        fs::read_to_string(&args.path).map_err(|err| Error::Internal(err.to_string()))?;
    let extension = file_extension(&args.path);
    let title = file_title(&args.path);

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    WidgetEditor::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let app_id = cnpy
        .core
        .create_detached(WidgetEditor::new(contents, extension, title));
    Root::install_with_inspector(&mut cnpy.core, app_id, false)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
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
