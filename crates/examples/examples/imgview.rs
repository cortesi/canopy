//! Launch the imgview example.

use std::{path::PathBuf, process};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    imgview::{create_app, setup_bindings},
    print_luau_api,
};
use canopy_widgets::{ImageView, Root};
use clap::Parser;

/// CLI flags for the imgview example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,

    /// Path to the image file to display.
    path: Option<PathBuf>,
}

/// Run the imgview example.
fn main() -> Result<()> {
    let args = Args::parse();

    if args.api {
        let mut cnpy = Canopy::new();
        Root::load(&mut cnpy)?;
        ImageView::load(&mut cnpy)?;
        setup_bindings(&mut cnpy);
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    let path = match args.path {
        Some(path) => path,
        None => {
            eprintln!("Usage: imgview <path>");
            return Ok(());
        }
    };
    let cnpy = create_app(&path)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
