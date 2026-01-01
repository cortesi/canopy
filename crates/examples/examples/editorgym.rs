//! Launch the editor gym example.

use std::io;

use canopy::{Canopy, Loader, backend::crossterm::runloop, error::Result};
use canopy_examples::editorgym::{EditorGym, setup_bindings};
use canopy_widgets::Root;
use clap::Parser;

/// CLI flags for the editor gym example.
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

/// Run the editor gym example.
fn main() -> Result<()> {
    let mut cnpy = Canopy::new();

    Root::load(&mut cnpy);
    EditorGym::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let args = Args::parse();
    if args.commands {
        cnpy.print_command_table(&mut io::stdout())?;
        return Ok(());
    }

    let app_id = cnpy.core.create_detached(EditorGym::new());
    Root::install_with_inspector(&mut cnpy.core, app_id, args.inspector)?;
    runloop(cnpy)?;
    Ok(())
}
