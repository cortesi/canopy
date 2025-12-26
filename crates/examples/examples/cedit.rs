//! Launch the cedit example.

use std::{env, error::Error, fs, result::Result};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::cedit::{Ed, setup_bindings};

/// Run the cedit example.
fn main() -> Result<(), Box<dyn Error>> {
    let filename = match env::args().nth(1) {
        Some(filename) => filename,
        None => {
            eprintln!("Usage: cedit <filename>");
            return Ok(());
        }
    };

    let mut cnpy = Canopy::new();
    Root::load(&mut cnpy);
    Ed::load(&mut cnpy);
    setup_bindings(&mut cnpy);

    let contents = fs::read_to_string(filename)?;
    let app_id = cnpy.core.add(Ed::new(&contents));
    Root::install_with_inspector(&mut cnpy.core, app_id, false)?;
    runloop(cnpy)?;
    Ok(())
}
