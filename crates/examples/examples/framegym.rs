//! Launch the frame gym example.

use std::{io, process};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::framegym::{FrameGym, setup_bindings};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the frame gym example.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Print available commands and exit.
    #[clap(short, long)]
    commands: bool,

    /// Enable the inspector overlay.
    #[clap(short, long)]
    inspector: bool,
}

/// Run the frame gym example.
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    Root::load(&mut cnpy)?;
    FrameGym::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut io::stdout(), false)?;
        return Ok(());
    }

    let app_id = cnpy.core.create_detached(FrameGym::new());
    Root::install_with_inspector(&mut cnpy.core, app_id, args.inspector)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
