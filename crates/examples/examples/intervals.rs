//! Launch the intervals example.

use std::process;

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    intervals::{Intervals, setup_bindings},
    print_luau_api,
};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the intervals example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,
}

/// Run the intervals example.
fn main() -> Result<()> {
    let args = Args::parse();

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Intervals::load(&mut cnpy)?;
    setup_bindings(&mut cnpy)?;

    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    Root::install_app(&mut cnpy, Intervals::new())?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
