//! Launch the pager example.

use std::{error::Error, fs, process, result::Result};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    pager::{Pager, setup_bindings},
    print_luau_api,
};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the pager example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,

    /// Path to the file to page.
    filename: Option<String>,
}

/// Run the pager example.
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Pager::load(&mut cnpy)?;
    setup_bindings(&mut cnpy)?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    let filename = match args.filename {
        Some(filename) => filename,
        None => {
            eprintln!("Usage: pager <filename>");
            return Ok(());
        }
    };

    let contents = fs::read_to_string(filename)?;
    Root::install_app(&mut cnpy, Pager::new(&contents))?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
