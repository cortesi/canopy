//! Launch the imgview example.

use std::{path::PathBuf, process};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::imgview::create_app;
use clap::Parser;

/// CLI flags for the imgview example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the image file to display.
    path: PathBuf,
}

/// Run the imgview example.
fn main() -> Result<()> {
    let args = Args::parse();
    let cnpy = create_app(&args.path)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
