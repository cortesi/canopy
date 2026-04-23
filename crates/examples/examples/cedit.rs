//! Launch the cedit example.

use std::{error::Error, fs, path::Path, process, result::Result};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    cedit::{Ed, setup_bindings},
    print_luau_api,
};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the cedit example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,

    /// Path to the file to edit.
    filename: Option<String>,
}

/// Run the cedit example.
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Ed::load(&mut cnpy)?;
    setup_bindings(&mut cnpy)?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    let filename = match args.filename {
        Some(filename) => filename,
        None => {
            eprintln!("Usage: cedit <filename>");
            return Ok(());
        }
    };

    let contents = fs::read_to_string(&filename)?;
    let extension = file_extension(&filename);
    let app_id = cnpy.core.create_detached(Ed::new(&contents, &extension));
    Root::install_with_inspector(&mut cnpy.core, app_id, false)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}

/// Return a lowercase file extension hint for syntax selection.
fn file_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "txt".to_string())
}
