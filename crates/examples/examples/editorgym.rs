//! Launch the editor gym example.

use std::process;

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::{
    editorgym::{EditorGym, setup_bindings},
    print_luau_api,
};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the editor gym example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print the Luau API definition and exit.
    #[clap(long)]
    api: bool,

    /// Enable the inspector overlay.
    #[clap(short, long)]
    inspector: bool,
}

/// Run the editor gym example.
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    Root::load(&mut cnpy)?;
    EditorGym::load(&mut cnpy)?;
    setup_bindings(&mut cnpy)?;

    let args = Args::parse();
    if args.api {
        print_luau_api(&mut cnpy)?;
        return Ok(());
    }

    Root::install_app_with_inspector(&mut cnpy, EditorGym::new(), args.inspector)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
