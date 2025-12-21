//! Launch the frame gym example.

use std::io;

use canopy::{Canopy, Loader, backend::crossterm::runloop, error::Result, widgets::Root};
use canopy_examples::framegym::{FrameGym, setup_bindings};
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
pub fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    Root::<FrameGym>::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut io::stdout())?;
        return Ok(());
    }

    runloop(
        cnpy,
        Root::new(FrameGym::new()).with_inspector(args.inspector),
    )?;
    Ok(())
}
