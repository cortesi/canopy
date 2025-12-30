//! Launch the chargym example.

use std::io;

use canopy::{Canopy, Loader, backend::crossterm::runloop, error::Result, widgets::Root};
use canopy_examples::chargym::{CharGym, setup_bindings};
use clap::Parser;

/// CLI flags for the chargym example.
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

/// Run the chargym example.
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy);
    CharGym::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut io::stdout())?;
        return Ok(());
    }

    let app_id = cnpy.core.add(CharGym::new());
    Root::install_with_inspector(&mut cnpy.core, app_id, args.inspector)?;
    runloop(cnpy)?;
    Ok(())
}
