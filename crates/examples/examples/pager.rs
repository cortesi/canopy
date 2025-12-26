//! Launch the pager example.

use std::{env, error::Error, fs, result::Result};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::pager::{Pager, setup_bindings};

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
    Root::load(&mut cnpy);
    Pager::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let contents = fs::read_to_string(filename)?;
    let app_id = cnpy.core.add(Pager::new(&contents));
    Root::install(&mut cnpy.core, app_id)?;
    runloop(cnpy)?;
    Ok(())
}
