//! Launch the pager example.

use std::{env, error::Error, fs, result::Result as StdResult};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::pager::{Pager, setup_bindings};

/// Run the pager example.
pub fn main() -> StdResult<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: pager filename");
    } else {
        let mut cnpy = Canopy::new();
        Root::load(&mut cnpy);
        Pager::load(&mut cnpy);
        setup_bindings(&mut cnpy);

        let contents = fs::read_to_string(args[1].clone())?;
        let app_id = cnpy.core.add(Pager::new(&contents));
        Root::install(&mut cnpy.core, app_id)?;
        runloop(cnpy)?;
    }
    Ok(())
}
