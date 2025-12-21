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
        Root::<Pager>::load(&mut cnpy);
        setup_bindings(&mut cnpy);

        let contents = fs::read_to_string(args[1].clone())?;
        let root = Root::new(Pager::new(&contents));
        runloop(cnpy, root)?;
    }
    Ok(())
}
