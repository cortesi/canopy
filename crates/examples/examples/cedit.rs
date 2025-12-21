//! Launch the cedit example.

use std::{env, error::Error, fs, result::Result as StdResult};

use canopy::{Canopy, Loader, backend::crossterm::runloop, widgets::Root};
use canopy_examples::cedit::{Ed, setup_bindings};

/// Run the cedit example.
pub fn main() -> StdResult<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: cedit filename");
    } else {
        let mut cnpy = Canopy::new();
        Root::<Ed>::load(&mut cnpy);
        setup_bindings(&mut cnpy);

        let contents = fs::read_to_string(args[1].clone())?;
        runloop(cnpy, Root::new(Ed::new(&contents)).with_inspector(false))?;
    }
    Ok(())
}
