//! Launch the pager example.

use std::{env, error::Error, fs, process, result::Result};

use canopy::{
    backend::crossterm::{RunloopOptions, runloop_with_options},
    prelude::*,
};
use canopy_examples::pager::{Pager, setup_bindings};
use canopy_widgets::Root;

/// Run the pager example.
fn main() -> Result<(), Box<dyn Error>> {
    let filename = match env::args().nth(1) {
        Some(filename) => filename,
        None => {
            eprintln!("Usage: pager <filename>");
            return Ok(());
        }
    };

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy)?;
    Pager::load(&mut cnpy)?;
    setup_bindings(&mut cnpy);

    let contents = fs::read_to_string(filename)?;
    let app_id = cnpy.core.create_detached(Pager::new(&contents));
    Root::install(&mut cnpy.core, app_id)?;
    let exit_code = runloop_with_options(cnpy, RunloopOptions::ctrlc_dump())?;
    if exit_code != 0 {
        process::exit(exit_code);
    }
    Ok(())
}
